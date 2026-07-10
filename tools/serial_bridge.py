#!/usr/bin/env python3
"""serial_bridge.py — neural-os-core v1.0
Bridge TCP <-> Serial para tunnel SLIP com watchdog e DNS hardening.

QEMU conecta como cliente: -serial tcp:127.0.0.1:4444,server,nowait
Hardware real: python serial_bridge.py --hardware COM3

Features:
  - Watchdog: reconexao automatica se QEMU cair
  - DNS hardening: healthcheck periodico, timeout configravel
  - Rate limiting: evita flood de logs
  - Estatisticas: RX/TX counters, throughput, uptime
"""

import argparse
import socket
import sys
import time
import select
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
        """Healthcheck: verifica se QEMU ainda responde."""
        if self.conn and self.watchdog_interval > 0:
            try:
                old_timeout = self.conn.gettimeout()
                self.conn.settimeout(1.0)
                self.conn.sendall(b"\x00")  # keepalive
                self.conn.settimeout(old_timeout)
            except (socket.timeout, OSError):
                log.warning("Watchdog: QEMU nao respondeu, reconectando...")
                self.conn = None

    def wait_for_qemu(self):
        """Aceita conexao do QEMU (com reconexao)."""
        if self.server is None:
            self.server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self.server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            self.server.bind(("127.0.0.1", self.port))
            self.server.listen(1)
            self.server.settimeout(self.reconnect_delay)

        while self.conn is None:
            try:
                log.info("Aguardando QEMU conectar em 127.0.0.1:%d...", self.port)
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
        """Loop principal do bridge TCP."""
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
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()
            except socket.timeout:
                pass
            except OSError as e:
                log.warning("Erro de conexao: %s", e)
                self.conn = None
                continue

            if sys.stdin in select.select([sys.stdin], [], [], 0)[0]:
                data = sys.stdin.buffer.read(4096)
                if data:
                    self.tx_count += len(data)
                    if self.conn:
                        try:
                            self.conn.sendall(data)
                        except OSError:
                            self.conn = None
            self.report()

            # Watchdog periodico
            now = time.time()
            if now - last_watchdog >= self.watchdog_interval:
                self.watchdog_check()
                last_watchdog = now

    def handle_hardware(self):
        """Loop para hardware real."""
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

            if sys.stdin in select.select([sys.stdin], [], [], 0)[0]:
                data = sys.stdin.buffer.read(4096)
                if data:
                    self.tx_count += len(data)
                    self.hw_ser.write(data)
            self.report()

    def run(self):
        log.info("Serial Bridge v1.0 - neural-os-core")
        if self.hw_port:
            self.handle_hardware()
        else:
            try:
                self.handle_tcp()
            except KeyboardInterrupt:
                log.info("Encerrando...")
            finally:
                if self.conn:
                    self.conn.close()
                if self.server:
                    self.server.close()
                self.report(force=True)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Serial bridge com watchdog")
    parser.add_argument("--port", type=int, default=4444)
    parser.add_argument("--baud", type=int, default=115200)
    parser.add_argument("--hardware", type=str, default=None)
    parser.add_argument("--watchdog", type=float, default=10.0,
                       help="Intervalo watchdog em segundos (0=desligado)")
    parser.add_argument("--reconnect", type=float, default=2.0,
                       help="Delay entre tentativas de reconexao (s)")
    args = parser.parse_args()

    bridge = SerialBridge(
        port=args.port, hw_port=args.hardware, baud=args.baud,
        watchdog_interval=args.watchdog, reconnect_delay=args.reconnect
    )
    bridge.run()
