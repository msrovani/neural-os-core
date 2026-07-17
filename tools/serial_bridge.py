#!/usr/bin/env python3
"""serial_bridge.py - neural-os-core serial/SLIP TCP peer (bypass de NIC emulada).

Topologia correta (inversao B-01 / CHANGELOG):
  - ESTE script = TCP servidor em 127.0.0.1:4444
  - QEMU = TCP cliente:  -serial tcp:127.0.0.1:4444
    (SEM server=on - se QEMU for server, disputa a mesma porta)

Ordem: python tools/serial_bridge.py  ->  .\\run-qemu-whpx.ps1
  (ou deixe o PS1 subir/derrubar o bridge automaticamente)

Hardware real: python tools/serial_bridge.py --hardware COM3
Deps: stdlib only (TCP). Hardware: pyserial.

Nota Windows: select() so funciona em sockets; stdin e tratado sem select.
Watchdog NAO injeta bytes no stream (evita corromper frames length-prefix).

Telemetria (ASCII-safe) vai para stderr - o PS1 redireciona para
logs/bridge_*.err.log (stdout = frames binarios em bridge_*.log).
"""

import argparse
import atexit
import socket
import sys
import time
import logging

logging.basicConfig(level=logging.INFO, format="[BRIDGE] %(message)s", stream=sys.stderr)
log = logging.getLogger("bridge")

# length-prefix BE u16 (ver slip.rs); MTU kernel = 1500
_FRAME_MTU = 1500
_REPORT_INTERVAL = 5.0


class SerialBridge:
    def __init__(self, port=4444, hw_port=None, baud=115200,
                 timeout=0.1, reconnect_delay=2.0, watchdog_interval=10.0):
        self.port = port
        self.hw_port = hw_port
        self.baud = baud
        self.timeout = timeout
        self.reconnect_delay = reconnect_delay
        self.watchdog_interval = watchdog_interval
        self.rx_count = 0
        self.tx_count = 0
        self.lifetime_rx = 0
        self.lifetime_tx = 0
        self.session_rx = 0
        self.session_tx = 0
        self.connect_events = 0
        self.disconnect_events = 0
        self.start_time = time.time()
        self.boot_time = self.start_time
        self.session_start = None
        self.last_report = 0
        self._prev_rx = 0
        self._prev_tx = 0
        self._prev_report_t = None
        self.last_frame_len = None
        self.last_chunk_rx = 0
        self.last_chunk_tx = 0
        self.state = "waiting"  # waiting | connected | disconnected
        self.peer = None  # "IP:port" or hardware path
        self.conn = None
        self.server = None
        self.hw_ser = None
        self._summary_done = False
        atexit.register(self._atexit_summary)

    @property
    def uptime(self):
        return time.time() - self.start_time

    def _peer_str(self):
        if self.peer:
            return self.peer
        return "-"

    def _session_elapsed(self):
        if self.session_start is None:
            return 0.0
        return time.time() - self.session_start

    def _rates(self, now):
        """Taxa media desde start + taxa aproximada no intervalo do report."""
        elapsed = now - self.start_time
        total = self.rx_count + self.tx_count
        avg = total / elapsed if elapsed > 0 else 0.0
        if self._prev_report_t is None:
            inst_rx = inst_tx = 0.0
        else:
            dt = now - self._prev_report_t
            if dt <= 0:
                inst_rx = inst_tx = 0.0
            else:
                inst_rx = (self.rx_count - self._prev_rx) / dt
                inst_tx = (self.tx_count - self._prev_tx) / dt
        return avg, inst_rx, inst_tx

    def _note_frame_len(self, data):
        """Se o chunk comeca com length-prefix BE u16 plausivel, guarda (STATS)."""
        if len(data) < 2:
            return
        flen = (data[0] << 8) | data[1]
        if 1 <= flen <= _FRAME_MTU:
            self.last_frame_len = flen

    def report(self, force=False, reason="periodic"):
        now = time.time()
        if not force and now - self.last_report < _REPORT_INTERVAL:
            return
        avg, inst_rx, inst_tx = self._rates(now)
        elapsed = now - self.start_time
        total = self.rx_count + self.tx_count
        flen = self.last_frame_len if self.last_frame_len is not None else "-"
        log.info(
            "STATS reason=%s state=%s peer=%s "
            "RX=%d TX=%d total=%d "
            "avg=%.0fB/s rx_rate=%.0fB/s tx_rate=%.0fB/s "
            "last_chunk_rx=%d last_chunk_tx=%d last_frame_len=%s "
            "uptime=%.0fs session=%.0fs connects=%d disconnects=%d",
            reason, self.state, self._peer_str(),
            self.rx_count, self.tx_count, total,
            avg, inst_rx, inst_tx,
            self.last_chunk_rx, self.last_chunk_tx, flen,
            elapsed, self._session_elapsed(),
            self.connect_events, self.disconnect_events,
        )
        self._prev_rx = self.rx_count
        self._prev_tx = self.tx_count
        self._prev_report_t = now
        self.last_report = now

    def summary(self, reason="shutdown"):
        if self._summary_done:
            return
        self._summary_done = True
        now = time.time()
        elapsed = max(now - self.boot_time, 0.001)
        life_rx = self.lifetime_rx + self.session_rx
        life_tx = self.lifetime_tx + self.session_tx
        total = life_rx + life_tx
        log.info(
            "SUMMARY reason=%s state=%s peer=%s "
            "lifetime_RX=%d lifetime_TX=%d total=%d avg=%.1fB/s uptime=%.1fs "
            "session_RX=%d session_TX=%d connects=%d disconnects=%d "
            "last_frame_len=%s",
            reason, self.state, self._peer_str(),
            life_rx, life_tx, total, total / elapsed, elapsed,
            self.session_rx, self.session_tx,
            self.connect_events, self.disconnect_events,
            self.last_frame_len if self.last_frame_len is not None else "-",
        )

    def _atexit_summary(self):
        try:
            self.summary(reason="atexit")
        except Exception:
            pass

    def watchdog_check(self):
        """Liveness sem injetar bytes no stream serial/SLIP."""
        if self.conn and self.watchdog_interval > 0:
            try:
                self.conn.getpeername()
            except OSError:
                log.warning(
                    "EVENT disconnect reason=watchdog peer=%s "
                    "session_RX=%d session_TX=%d",
                    self._peer_str(), self.session_rx, self.session_tx,
                )
                try:
                    self.conn.close()
                except OSError:
                    pass
                self.conn = None
                self.state = "disconnected"
                self.disconnect_events += 1
                self.report(force=True, reason="watchdog")
                self.peer = None
                self.state = "waiting"

    def _stdin_ready(self):
        """True se stdin tem dados. Em Windows select() nao aceita pipes/console."""
        if sys.platform == "win32":
            try:
                import msvcrt
                if sys.stdin.isatty():
                    return msvcrt.kbhit()
            except Exception:
                return False
            return False
        try:
            import select
            return bool(select.select([sys.stdin], [], [], 0)[0])
        except (OSError, ValueError):
            return False

    def wait_for_qemu(self):
        """Aceita conexao do QEMU (cliente) com reconexao."""
        if self.server is None:
            self.server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self.server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self.server.bind(("127.0.0.1", self.port))
            self.server.listen(1)
            self.server.settimeout(self.reconnect_delay)
            self.state = "waiting"
            self.peer = None
            log.info(
                "EVENT listen state=waiting bind=127.0.0.1:%d "
                "(aguarde QEMU -serial tcp:127.0.0.1:%d)",
                self.port, self.port,
            )
            self.report(force=True, reason="listen")

        while self.conn is None:
            self.state = "waiting"
            try:
                conn, addr = self.server.accept()
                peer = "%s:%d" % (addr[0], addr[1])
                log.info(
                    "EVENT connect state=connected peer=%s "
                    "(QEMU client ESTABLISHED)",
                    peer,
                )
                conn.settimeout(self.timeout)
                self.conn = conn
                self.peer = peer
                self.state = "connected"
                self.connect_events += 1
                self.lifetime_rx += self.session_rx
                self.lifetime_tx += self.session_tx
                self.rx_count = 0
                self.tx_count = 0
                self.session_rx = 0
                self.session_tx = 0
                self.last_chunk_rx = 0
                self.last_chunk_tx = 0
                self.last_frame_len = None
                self.start_time = time.time()
                self.session_start = self.start_time
                self._prev_rx = 0
                self._prev_tx = 0
                self._prev_report_t = self.start_time
                self.report(force=True, reason="connect")
            except socket.timeout:
                # heartbeat enquanto LISTEN vazio
                self.report(reason="waiting")

    def handle_tcp(self):
        """Loop principal: peer TCP <-> stdout/stdin (pipe de frames)."""
        last_watchdog = 0
        while True:
            if self.conn is None:
                self.wait_for_qemu()
                continue
            try:
                data = self.conn.recv(4096)
                if not data:
                    log.warning(
                        "EVENT disconnect reason=eof peer=%s "
                        "session_RX=%d session_TX=%d",
                        self._peer_str(), self.session_rx, self.session_tx,
                    )
                    try:
                        self.conn.close()
                    except OSError:
                        pass
                    self.conn = None
                    self.state = "disconnected"
                    self.disconnect_events += 1
                    self.report(force=True, reason="disconnect")
                    self.peer = None
                    self.state = "waiting"
                    continue
                n = len(data)
                self.rx_count += n
                self.session_rx += n
                self.last_chunk_rx = n
                self._note_frame_len(data)
                try:
                    sys.stdout.buffer.write(data)
                    sys.stdout.buffer.flush()
                except (BrokenPipeError, OSError):
                    pass
            except socket.timeout:
                pass
            except OSError as e:
                log.warning(
                    "EVENT disconnect reason=error err=%s peer=%s "
                    "session_RX=%d session_TX=%d",
                    e, self._peer_str(), self.session_rx, self.session_tx,
                )
                try:
                    self.conn.close()
                except OSError:
                    pass
                self.conn = None
                self.state = "disconnected"
                self.disconnect_events += 1
                self.report(force=True, reason="error")
                self.peer = None
                self.state = "waiting"
                continue

            if self._stdin_ready():
                data = sys.stdin.buffer.read(4096)
                if data:
                    n = len(data)
                    self.tx_count += n
                    self.session_tx += n
                    self.last_chunk_tx = n
                    self._note_frame_len(data)
                    if self.conn:
                        try:
                            self.conn.sendall(data)
                        except OSError as e:
                            log.warning(
                                "EVENT disconnect reason=send_error err=%s peer=%s",
                                e, self._peer_str(),
                            )
                            self.conn = None
                            self.state = "disconnected"
                            self.disconnect_events += 1
                            self.report(force=True, reason="send_error")
                            self.peer = None
                            self.state = "waiting"
            self.report()

            now = time.time()
            if self.watchdog_interval > 0 and now - last_watchdog >= self.watchdog_interval:
                self.watchdog_check()
                last_watchdog = now

    def handle_hardware(self):
        """Loop para hardware real (COM* via pyserial)."""
        try:
            import serial
            self.hw_ser = serial.Serial(self.hw_port, self.baud, timeout=self.timeout)
            self.peer = self.hw_port
            self.state = "connected"
            self.connect_events += 1
            self.session_start = time.time()
            log.info(
                "EVENT connect state=connected peer=%s baud=%d (hardware)",
                self.hw_port, self.baud,
            )
            self.report(force=True, reason="hw_connect")
        except ImportError:
            log.error("pyserial nao instalado: pip install pyserial")
            return

        while True:
            if self.hw_ser.in_waiting:
                data = self.hw_ser.read(self.hw_ser.in_waiting)
                n = len(data)
                self.rx_count += n
                self.session_rx += n
                self.last_chunk_rx = n
                self._note_frame_len(data)
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()

            if self._stdin_ready():
                data = sys.stdin.buffer.read(4096)
                if data:
                    n = len(data)
                    self.tx_count += n
                    self.session_tx += n
                    self.last_chunk_tx = n
                    self._note_frame_len(data)
                    self.hw_ser.write(data)
            self.report()

    def run(self):
        log.info(
            "Serial Bridge v1.2 - neural-os-core "
            "(TCP server / QEMU client; telemetria em stderr)"
        )
        if self.hw_port:
            try:
                self.handle_hardware()
            except KeyboardInterrupt:
                log.info("EVENT shutdown reason=KeyboardInterrupt")
            finally:
                self.state = "disconnected" if self.state == "connected" else self.state
                self.summary(reason="shutdown")
        else:
            try:
                self.handle_tcp()
            except KeyboardInterrupt:
                log.info("EVENT shutdown reason=KeyboardInterrupt")
            finally:
                if self.conn:
                    try:
                        self.conn.close()
                    except OSError:
                        pass
                if self.server:
                    try:
                        self.server.close()
                    except OSError:
                        pass
                if self.state == "connected":
                    self.state = "disconnected"
                self.summary(reason="shutdown")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Serial bridge SLIP/TCP peer para QEMU COM2")
    parser.add_argument("--port", type=int, default=4444,
                        help="Porta TCP local (default 4444; QEMU deve conectar como cliente)")
    parser.add_argument("--baud", type=int, default=115200)
    parser.add_argument("--hardware", type=str, default=None)
    parser.add_argument("--watchdog", type=float, default=10.0,
                        help="Intervalo watchdog em segundos (0=desligado)")
    parser.add_argument("--reconnect", type=float, default=2.0,
                        help="Delay entre accept timeouts (s)")
    args = parser.parse_args()

    bridge = SerialBridge(
        port=args.port, hw_port=args.hardware, baud=args.baud,
        watchdog_interval=args.watchdog, reconnect_delay=args.reconnect
    )
    bridge.run()
