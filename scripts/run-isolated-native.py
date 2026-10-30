#!/usr/bin/env python3
"""Local development helper; accepts only a pre-created isolated test home.

No provider credentials are inherited. This starts the actual Rust binary, not
an HTML mock. Run the project's normal executable for everyday usage instead.
"""
import argparse
import os
from pathlib import Path



def build_environment(home: Path, inspector_port: int, inherited) -> dict[str, str]:
    state = home.parent / "native-smoke-state"
    allowed = ("PATH", "LANG", "LC_ALL", "DISPLAY", "WAYLAND_DISPLAY",
               "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "TMPDIR")
    env = {key: inherited[key] for key in allowed if key in inherited}
    # HERMES_HOME alone does not isolate profile discovery or the host-wide owner.
    env.update(HOME=str(state / "os-home"),
               XDG_CONFIG_HOME=str(state / "config"),
               XDG_DATA_HOME=str(state / "data"), XDG_CACHE_HOME=str(state / "cache"),
               XDG_STATE_HOME=str(state / "state"),
               HERMES_GATEWAY_LOCK_DIR=str(state / "gateway-locks"),
               WEBKIT_INSPECTOR_HTTP_SERVER=f"127.0.0.1:{inspector_port}",
               WEBKIT_INSPECTOR_SERVER=f"127.0.0.1:{inspector_port + 1}")
    return env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--home", required=True, type=Path)
    parser.add_argument("--hermes", required=True, type=Path)
    parser.add_argument("--renderer", type=Path)
    parser.add_argument("--inspector-port", type=int, default=19222)
    parser.add_argument("--no-auto-connect", action="store_true")
    args = parser.parse_args()
    home = args.home.resolve(strict=True)
    if home == Path.home() / ".hermes" or ".hermes/profiles/" in str(home):
        parser.error("Use a disposable test home, not a live Hermes profile")
    if not 1024 <= args.inspector_port <= 65534:
        parser.error("Inspector port must be in 1024..65534")
    root = Path(__file__).resolve().parents[1]
    env = build_environment(home, args.inspector_port, os.environ)
    for key in ("HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
                "XDG_STATE_HOME", "HERMES_GATEWAY_LOCK_DIR"):
        Path(env[key]).mkdir(parents=True, exist_ok=True, mode=0o700)
    command = [str(root / "target/debug/hermes-desktop-rust"), "--home", str(home),
               "--hermes", str(args.hermes.resolve(strict=True))]
    if args.renderer:
        command += ["--renderer", str(args.renderer.resolve(strict=True))]
    if args.no_auto_connect:
        command += ["--no-auto-connect"]
    print("Running the real native host with isolated app data; provider credentials stripped.", flush=True)
    os.execve(command[0], command, env)


if __name__ == "__main__":
    raise SystemExit(main())
