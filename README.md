# gamesir-firmware

Private research notes on GameSir controller firmware (JieLi AC695X) and the GameSir Nexus
Windows app. The goal is interoperability for OpenJoystickDriver, a macOS userspace driver. The
first target is the G7 SE, so that its modes and LED can be driven without Nexus.

- `AGENTS.md`: the brief, rules and open tasks.
- `notes/facts.md`: verified facts, with their source.
- `tools/`: scripts that work on the local `private/` copies.
- `private/`: vendor binaries, firmware, captures and probes. It is gitignored. Rebuild it from
  the sources listed in `AGENTS.md`.
