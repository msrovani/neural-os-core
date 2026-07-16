#!/usr/bin/env python3
"""serial_bridge.py — neural-os-core serial/SLIP TCP peer (bypass de NIC emulada).

Topologia correta (inversao B-01 / CHANGELOG):
  - ESTE script = TCP servidor em 127.0.0.1:4444
  - QEMU = TCP cliente:  -serial tcp:127.0.0.1:4444
    (SEM server=on — se QEMU for server, disputa a mesma porta)

Ordem: python tools/serial_bridge.py  →  .\run-qemu-whpx.ps1
  (ou deixe o PS1 subir/derrubar o bridge automaticamente)

Hardware real: python tools/serial_bridge.py --hardware COM3
Deps: stdlib only (TCP). Hardware: pyserial.

Nota Windows: select() so funciona em sockets; stdin e tratado sem select.
Watchdog NAO injeta bytes no stream (evita corromper frames length-prefix).
"""

import argparse
import socket
import sys
import time
import logging

logging.basicConfig(level=logging.INFO, format="[BRIDGE] %(message)s")
log = logging.getLogger("bridge")


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
        self.start_time = time.time()
        self.last_report = 0
        self.conn = None
        self.server = None
        self.hw_ser = None

    @property
    def uptime(self):
        return time.time() - self.start_time

    def report(self, force=False):
        now = time.time()
        if force or now - self.last_report >= 5.0:
            elapsed = now - self.start_time
            total = self.rx_count + self.tx_count
            rate = total / elapsed if elapsed > 0 else 0
            log.info("RX: %d  TX: %d  Total: %d  (%.0f B/s)  uptime: %.0fs",
                     self.rx_count, self.tx_count, total, rate, elapsed)
            self.last_report = now

    def watchdog_check(self):
        """Liveness sem injetar bytes no stream serial/SLIP."""
        if self.conn and self.watchdog_interval > 0:
            try:
                self.conn.getpeername()
            except OSError:
                log.warning("Watchdog: socket morto, reconectando...")
                try:
                    self.conn.close()
                except OSError:
                    pass
                self.conn = None

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
            log.info("LISTEN 127.0.0.1:%d (aguarde QEMU -serial tcp:127.0.0.1:%d)",
                     self.port, self.port)

        while self.conn is None:
            try:
                conn, addr = self.server.accept()
                log.info("QEMU conectado de %s", addr)
                conn.settimeout(self.timeout)
                self.conn = conn
                self.rx_count = 0
                self.tx_count = 0
                self.start_time = time.time()
            except socket.timeout:
                pass

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
                    log.warning("QEMU desconectou")
                    self.conn.close()
                    self.conn = None
                    continue
                self.rx_count += len(data)
                try:
                    sys.stdout.buffer.write(data)
                    sys.stdout.buffer.flush()
                except (BrokenPipeError, OSError):
                    pass
            except socket.timeout:
                pass
            except OSError as e:
                log.warning("Erro de conexao: %s", e)
                try:
                    self.conn.close()
                except OSError:
                    pass
                self.conn = None
                continue

            if self._stdin_ready():
                data = sys.stdin.buffer.read(4096)
                if data:
                    self.tx_count += len(data)
                    if self.conn:
                        try:
                            self.conn.sendall(data)
                        except OSError:
                            self.conn = None
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
            log.info("Hardware conectado em %s @ %d baud", self.hw_port, self.baud)
        except ImportError:
            log.error("pyserial nao instalado: pip install pyserial")
            return

        while True:
            if self.hw_ser.in_waiting:
                data = self.hw_ser.read(self.hw_ser.in_waiting)
                self.rx_count += len(data)
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()

            if self._stdin_ready():
                data = sys.stdin.buffer.read(4096)
                if data:
                    self.tx_count += len(data)
                    self.hw_ser.write(data)
            self.report()

    def run(self):
        log.info("Serial Bridge v1.1 - neural-os-core (TCP server / QEMU client)")
        if self.hw_port:
            self.handle_hardware()
        else:
            try:
                self.handle_tcp()
            except KeyboardInterrupt:
                log.info("Encerrando...")
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
                self.report(force=True)


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
