#!/usr/bin/env python3
"""Ferramenta legada de export Agency — ADR-0052.

NÃO regenera stubs em massa. Stubs copiados não são artefatos Neural OS.
Use este script apenas para inspecionar inventário histórico (--legacy-count)
ou para regenerar o seed NATIVO curated (tools/native_agents.toml → native_agent_seed.rs).

Agency seed permanece vazio (&[]) até haver AGENT.md reais assinados.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NATIVE_CONFIG = ROOT / "tools" / "native_agents.toml"
AGENCY_SEED = ROOT / "crates" / "k_ai" / "src" / "agency_seed.rs"
NATIVE_SEED = ROOT / "crates" / "k_ai" / "src" / "native_agent_seed.rs"
SPEC_RE = re.compile(
    r'spec\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,\s*"([^"]*)"\s*,\s*&\[(.*?)\]\s*\)',
    re.DOTALL,
)
STRING_RE = re.compile(r'"([^"]*)"')


@dataclass(frozen=True)
class Agent:
    name: str
    division: str
    mission: str
    skills: tuple[str, ...]
    tier: str = "agency"
    schedule: str = "EventDriven"
    native_impl: str = "SpecialistAgent"
    kind: str = "Skill"
    source: str = "agency"


def rust_str(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def git_text(relative_path: str) -> str | None:
    result = subprocess.run(
        ["git", "show", f"HEAD:{relative_path}"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if result.returncode != 0:
        return None
    return result.stdout


def parse_specs_text(text: str, source: str) -> list[Agent]:
    return [
        Agent(
            name=match.group(1),
            division=match.group(2),
            mission=match.group(3),
            skills=tuple(STRING_RE.findall(match.group(4))),
            source=source,
        )
        for match in SPEC_RE.finditer(text)
    ]


def load_agency_legacy_count() -> int:
    for agency_path, importer_path in (
        ("crates/neural-kernel/src/agency.rs", "crates/neural-kernel/src/agency_importer.rs"),
        ("crates/k_ai/src/agency.rs", "crates/k_ai/src/agency_importer.rs"),
    ):
        agency_src = git_text(agency_path)
        importer_src = git_text(importer_path)
        if agency_src is None or importer_src is None:
            continue
        agents = parse_specs_text(agency_src, "agency-base")
        agents.extend(parse_specs_text(importer_src, "agency-imported"))
        return len(agents)
    return 0


def load_native() -> list[Agent]:
    data = tomllib.loads(NATIVE_CONFIG.read_text(encoding="utf-8"))
    return [
        Agent(
            name=item["name"],
            division=item["division"],
            mission=item["mission"],
            skills=tuple(item.get("skills", [])),
            tier="native",
            schedule=item["schedule"],
            native_impl=item["native_impl"],
            kind=item["kind"],
            source="native",
        )
        for item in data["agent"]
    ]


def write_empty_agency_seed() -> None:
    AGENCY_SEED.write_text(
        "//! Seed Agency — ADR-0052: stubs copiados NÃO são artefatos.\n"
        "//! Fleet Agency só sobe via PackageHub com AGENT.md assinado + hash + acionaveis.\n"
        "//! Não regenerar em massa com export_agent_packages.py.\n\n"
        "pub struct AgentSeedRecord {\n"
        "    pub name: &'static str,\n"
        "    pub division: &'static str,\n"
        "    pub mission: &'static str,\n"
        "    pub skills: &'static [&'static str],\n"
        "}\n\n"
        "/// Vazio de propósito: SpecialistAgent stub sem missão executável = deny.\n"
        "pub const AGENCY_SEEDS: &[AgentSeedRecord] = &[];\n",
        encoding="utf-8",
    )


def write_native_seed(agents: list[Agent]) -> None:
    lines = [
        "//! Seed nativo curated (tools/native_agents.toml). Manifesto = catálogo;",
        "//! código Ring0/IRQ/HAL permanece no bin (ADR-0052 native_compiled).",
        "",
        "pub struct NativeAgentSeed {",
        "    pub name: &'static str,",
        "    pub division: &'static str,",
        "    pub mission: &'static str,",
        "    pub schedule: &'static str,",
        "    pub native_impl: &'static str,",
        "    pub kind: &'static str,",
        "    pub skills: &'static [&'static str],",
        "}",
        "",
        "pub const NATIVE_AGENT_SEEDS: &[NativeAgentSeed] = &[",
    ]
    for agent in agents:
        skills = ", ".join(rust_str(skill) for skill in agent.skills)
        lines.append(
            "    NativeAgentSeed { "
            f"name: {rust_str(agent.name)}, division: {rust_str(agent.division)}, "
            f"mission: {rust_str(agent.mission)}, schedule: {rust_str(agent.schedule)}, "
            f"native_impl: {rust_str(agent.native_impl)}, kind: {rust_str(agent.kind)}, "
            f"skills: &[{skills}] }},"
        )
    lines.extend(["];", ""])
    NATIVE_SEED.write_text("\n".join(lines), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--legacy-count",
        action="store_true",
        help="conta specs Agency no git HEAD (histórico; não gera stubs)",
    )
    parser.add_argument(
        "--refresh-native-seed",
        action="store_true",
        help="regenera apenas native_agent_seed.rs + agency_seed vazio",
    )
    parser.add_argument(
        "--force-stub-export",
        action="store_true",
        help="BLOQUEADO (ADR-0052): regenerar stubs em massa é deny",
    )
    args = parser.parse_args()

    if args.force_stub_export:
        print(
            "[DENY] ADR-0052: export em massa de AGENT.md stub é proibido.\n"
            "       Crie artefatos individuais com schema/hash/signature/acionaveis.",
            file=sys.stderr,
        )
        raise SystemExit(2)

    if args.legacy_count:
        n = load_agency_legacy_count()
        print(f"[INFO] Agency histórico (git HEAD specs): {n}")
        print("[INFO] Runtime atual: AGENCY_SEEDS=&[] — fleet Agency=0 até pacotes assinados")
        return

    if args.refresh_native_seed:
        write_empty_agency_seed()
        native = load_native()
        write_native_seed(native)
        print(f"[OK] agency_seed vazio; native_agent_seed={len(native)}")
        return

    print(
        "[DENY] Por padrão este script não gera stubs (ADR-0052).\n"
        "       Use --legacy-count ou --refresh-native-seed.",
        file=sys.stderr,
    )
    raise SystemExit(2)


if __name__ == "__main__":
    main()
