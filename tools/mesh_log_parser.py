#!/usr/bin/env python3
"""
Parser de marcos de log QEMU mesh - Etapa 1
Procura substrings nos logs boot_mesh_a/b.txt e reporta GOALs.
Uso: python tools/mesh_log_parser.py [logs/boot_mesh_a.txt] [timeout_s]
Retorna JSON com marcos encontrados.
"""
import re, sys, json, pathlib

MARCOS = {
    "boot_phase": [r"\[BOOT\] Phase", r"Phase 0.*SafeHarbor", r"Phase 7.*Runtime", r"Phase 8"],
    "timer_tick": [r"\[TIMER\] tick="],
    "display": [r"DisplayAgent", r"compositor", r"Framebuffer", r"UI_SPEC", r"CardWindow"],
    "audio": [r"Piper", r"TTS", r"wake word.*Jarvis", r"Jarvis", r"HDA", r"AUDIO_IN"],
    "orb": [r"orb", r"FFT", r"Goertzel", r"AffectVector"],
    "mesh_engine": [r"MESH_ENGINE", r"mesh", r"UDP.*42069", r"P2P", r"p2p_tick"],
    "election": [r"Master", r"Worker", r"elei.*", r"ROLE", r"become.*master", r"become.*worker"],
    "skill_sync": [r"SkillSync", r"skill.*sync", r"P2P_PACKET", r"push.*skill"],
    "marketplace": [r"marketplace", r"MKTP", r"MICROPY", r"micropython", r"python_eval"],
    "smp": [r"SMP", r"AP.*wake", r"INIT.*SIPI", r"AP_ENTRY"],
    "net": [r"e1000", r"netdev", r"smoltcp", r"10\.0\.3\.[23]"],
    "errors": [r"#PF", r"#GP", r"PANIC", r"triple fault", r"Triple fault", r"FAIL", r"ERRO"],
}

def parse_log(path):
    p = pathlib.Path(path)
    if not p.exists():
        return {"file": str(p), "exists": False, "size": 0, "lines": 0, "marcos": {}}
    text = p.read_text(errors="ignore")
    lines = text.splitlines()
    result = {"file": str(p), "exists": True, "size": len(text), "lines": len(lines), "marcos": {}, "last_tick": None, "last_lines": lines[-20:] if lines else []}
    for name, patterns in MARCOS.items():
        hits = []
        for pat in patterns:
            for i, line in enumerate(lines):
                if re.search(pat, line, re.IGNORECASE):
                    hits.append({"line": i+1, "pattern": pat, "text": line[:300]})
                    if len(hits) >= 5:
                        break
            if hits:
                break
        result["marcos"][name] = hits[:5]
    # last timer tick
    for line in reversed(lines):
        m = re.search(r"tick=(\d+)", line)
        if m:
            result["last_tick"] = int(m.group(1))
            break
    # GOAL evaluation
    boot_ok = bool(result["marcos"]["boot_phase"] and result["marcos"]["timer_tick"])
    ui_ok = bool(result["marcos"]["display"])
    mesh_ok = bool(result["marcos"]["mesh_engine"])
    election_ok = bool(result["marcos"]["election"])
    skillsync_ok = bool(result["marcos"]["skill_sync"])
    mktp_ok = bool(result["marcos"]["marketplace"])
    result["goals"] = {
        "GOAL1_boot": boot_ok,
        "GOAL1_ui": ui_ok,
        "GOAL2_election": mesh_ok and election_ok,
        "GOAL3_skillsync": skillsync_ok,
        "GOAL3_marketplace": mktp_ok,
    }
    return result

if __name__ == "__main__":
    paths = sys.argv[1:] if len(sys.argv) > 1 else ["logs/boot_mesh_a.txt", "logs/boot_mesh_b.txt"]
    out = [parse_log(p) for p in paths]
    print(json.dumps(out, indent=2, ensure_ascii=False))
