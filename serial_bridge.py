#!/usr/bin/env python3
"""serial_bridge.py — neural-os-core v1.0
Bridge TCP <-> Serial para tunnel SLIP.
QEMU conecta como cliente a esta porta via -serial tcp:127.0.0.1:4444,server,nowait

Uso: python serial_bridge.py [--port 4444] [--baud 115200]
      python serial_bridge.py --hardware COM3  # para hardware real via USB-serial
"""

import argparse
import socket
import sys
import time

def run_tcp_server(port: int):
    """Aceita conexao do QEMU e faz bridge stdout/stdin"""
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", port))
    server.listen(1)
    server.settimeout(None)  # blocking accept

    print(f"[BRIDGE] Aguardando QEMU conectar em 127.0.0.1:{port}...")
    conn, addr = server.accept()
    print(f"[BRIDGE] QEMU conectado de {addr}")
    conn.settimeout(0.1)

    rx_count = 0
    tx_count = 0
    last_report = time.time()

    try:
        while True:
            # QEMU -> stdout (RX do kernel)
            try:
                data = conn.recv(4096)
                if not data:
                    print("[BRIDGE] QEMU desconectou")
                    break
                rx_count += len(data)
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()
            except socket.timeout:
                pass

            # stdin -> QEMU (TX para o kernel)
            if sys.stdin in select.select([sys.stdin], [], [], 0)[0]:
                data = sys.stdin.buffer.read(4096)
                if not data:
                    break
                tx_count += len(data)
                conn.sendall(data)

            # Report a cada 5s
            now = time.time()
            if now - last_report >= 5.0:
                print(f"\r[BRIDGE] RX: {rx_count}B  TX: {tx_count}B  Total: {rx_count+tx_count}B", end="", flush=True)
                last_report = now

    except KeyboardInterrupt:
        print("\n[BRIDGE] Encerrando...")
    finally:
        conn.close()
        server.close()
        print(f"[BRIDGE] Finalizado. RX: {rx_count}B  TX: {tx_count}B")


def run_hardware(port: str, baud: int):
    """Bridge com hardware real via porta serial USB"""
    import serial
    ser = serial.Serial(port, baud, timeout=0.1)
    print(f"[BRIDGE] Hardware conectado em {port} @ {baud} baud")

    rx_count = 0
    tx_count = 0
    last_report = time.time()

    try:
        while True:
            if ser.in_waiting:
                data = ser.read(ser.in_waiting)
                rx_count += len(data)
                sys.stdout.buffer.write(data)
                sys.stdout.buffer.flush()

            if sys.stdin in select.select([sys.stdin], [], [], 0)[0]:
                data = sys.stdin.buffer.read(4096)
                if not data:
                    break
                tx_count += len(data)
                ser.write(data)

            now = time.time()
            if now - last_report >= 5.0:
                print(f"\r[BRIDGE] RX: {rx_count}B  TX: {tx_count}B", end="", flush=True)
                last_report = now
    except KeyboardInterrupt:
        print("\n[BRIDGE] Encerrando...")
    finally:
        ser.close()


if __name__ == "__main__":
    import select  # usado em ambos modos

    parser = argparse.ArgumentParser(description="Serial bridge para neural-os-core")
    parser.add_argument("--port", type=int, default=4444, help="Porta TCP (default: 4444)")
    parser.add_argument("--baud", type=int, default=115200, help="Baud rate para HW (default: 115200)")
    parser.add_argument("--hardware", type=str, default=None, help="Porta serial HW ex: COM3")
    args = parser.parse_args()

    if args.hardware:
        run_hardware(args.hardware, args.baud)
    else:
        run_tcp_server(args.port)
