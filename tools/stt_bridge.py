#!/usr/bin/env python3
"""stt_bridge.py — neural-os-core v1.0
Bridge STT (Speech-to-Text) via serial tunnel.
Kernel envia audio PCM 16kHz → bridge reconhece → retorna texto.

Uso: python tools/stt_bridge.py [--port 4445] [--backend whisper]
      python tools/stt_bridge.py --backend sensevoice  # requer SenseVoice API
      python tools/stt_bridge.py --backend dummy       # modo debug (eco)

Requisitos (whisper): pip install faster-whisper
Requisitos (sensevoice): servidor SenseVoice em http://localhost:50000
"""

import argparse
import json
import socket
import struct
import sys
import time
import threading

BACKEND = None

def select_backend(name):
    global BACKEND
    if name == "whisper":
        try:
            from faster_whisper import WhisperModel
            BACKEND = WhisperModel("base", device="cpu", compute_type="int8")
            print("[STT] Whisper base carregado (CPU int8)")
        except ImportError:
            print("[STT] faster-whisper nao instalado. pip install faster-whisper")
            sys.exit(1)
    elif name == "sensevoice":
        BACKEND = "sensevoice"
        print("[STT] SenseVoice via API em http://localhost:50000")
    else:
        BACKEND = None
        print("[STT] Modo dummy: ecoa texto de teste")

def transcribe(audio_bytes):
    """Transcreve audio PCM 16kHz 16-bit mono."""
    if BACKEND is None:
        return "[stt_dummy: audio reconhecido]"
    if BACKEND == "sensevoice":
        try:
            import urllib.request
            import tempfile
            with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as f:
                import wave
                with wave.open(f, "wb") as wf:
                    wf.setnchannels(1)
                    wf.setsampwidth(2)
                    wf.setframerate(16000)
                    wf.writeframes(audio_bytes)
                fname = f.name
            # Envia para SenseVoice API
            with open(fname, "rb") as f:
                data = f.read()
            req = urllib.request.Request(
                "http://localhost:50000/api/v1/asr",
                data=b"--boundary\r\nContent-Disposition: form-data; name=\"files\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n" + data + b"\r\n--boundary--\r\n",
                headers={"Content-Type": "multipart/form-data; boundary=boundary"}
            )
            resp = urllib.request.urlopen(req, timeout=5)
            result = json.loads(resp.read())
            if result.get("result"):
                return result["result"][0].get("clean_text", result["result"][0].get("text", ""))
            return ""
        except Exception as e:
            return f"[stt_error: {e}]"
    else:
        # Whisper
        import numpy as np
        samples = np.frombuffer(audio_bytes, dtype=np.int16).astype(np.float32) / 32768.0
        segments, _ = BACKEND.transcribe(samples, beam_size=5)
        text = " ".join(seg.text for seg in segments)
        return text.strip() or "[silence]"

def handle_client(conn):
    """Processa requisicoes STT do kernel via TCP."""
    buf = bytearray()
    while True:
        try:
            data = conn.recv(4096)
            if not data:
                break
            buf.extend(data)
            # Formato: length_prefix(4 bytes) + pcm_data
            while len(buf) >= 4:
                length = struct.unpack("<I", buf[:4])[0]
                if length == 0:
                    buf = buf[4:]
                    continue
                if len(buf) < 4 + length:
                    break
                audio = bytes(buf[4:4 + length])
                buf = buf[4 + length:]
                text = transcribe(audio)
                resp = struct.pack("<I", len(text)) + text.encode("utf-8")
                conn.sendall(resp)
        except (socket.timeout, ConnectionError):
            break
    conn.close()

def run_server(port):
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", port))
    server.listen(5)
    print(f"[STT] Servidor aguardando conexoes em 127.0.0.1:{port}")
    while True:
        conn, addr = server.accept()
        print(f"[STT] Conexao de {addr}")
        conn.settimeout(30)
        t = threading.Thread(target=handle_client, args=(conn,), daemon=True)
        t.start()

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=4445)
    parser.add_argument("--backend", choices=["whisper", "sensevoice", "dummy"], default="dummy")
    args = parser.parse_args()
    select_backend(args.backend)
    run_server(args.port)
