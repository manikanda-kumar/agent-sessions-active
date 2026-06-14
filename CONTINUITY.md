Goal (incl. success criteria):
- Add repo support for additional coding agents: Codex, Amp, Pi, and Droid.
- Success means active sessions can be detected, typed, displayed in the UI, and covered by build/tests.

Constraints/Assumptions:
- Existing Claude/OpenCode behavior should remain unchanged.
- Codex sessions are read from `~/.codex/sessions/YYYY/MM/DD/*.jsonl`.
- Amp sessions are read from `~/.local/share/amp/threads/*.json` and matched to live `amp` processes by `env.initial.trees[].uri` cwd when available, with latest-unused recency fallback.
- Pi sessions are read from `~/.pi/agent/sessions/*/*.jsonl` using Pi's `--Users-...--` project directory encoding.
- Droid sessions are read from Factory's `~/.factory/sessions` JSONL store; session IDs come from `type:"session_start"` records.

Key decisions:
- Added focused detectors instead of forcing all agents through the Claude parser.
- Reused shared process snapshot and common CLI process matching for new CLI agents.
- Used compact letter icons for non-Claude/OpenCode agents to avoid adding new assets.
- Compared cmux Amp support: cmux installs an Amp plugin at `~/.config/amp/plugins/cmux-session.ts` to receive native Amp lifecycle/status events and hook them into `~/.cmuxterm/amp-hook-sessions.json`.

State:
- Implementation complete, packaged, and smoke-tested from an installed macOS app.

Done:
- Added Rust `AgentType` variants for `codex`, `amp`, `pi`, and `droid`.
- Added Codex, Amp, Pi/Droid JSONL-style, and generic process detector modules.
- Improved Amp detection using Amp thread workspace URI metadata discovered from local thread files and cmux reference work.
- Generalized the Claude-style parser to accept alternate session roots and Pi session IDs.
- Updated frontend TypeScript agent union and session card rendering.
- Updated README supported agents and empty-state copy.
- Added Pi parser tests.
- Confirmed Factory Droid sessions under `~/.factory/sessions` and updated Droid detection/parser support for `session_start` records.
- Ran `cargo test`, `npm test -- --run`, and `npm run build` successfully.
- Built macOS `.app` and `.dmg`; installed side-by-side as `/Applications/Agent Sessions 0.2.1.app` and launched it successfully.
- Added a root `Makefile` with install, dev, build, test, Tauri build, DMG, DMG install, launch, and clean targets.

Now:
- Ready for user review with installable DMG and Makefile build commands available.

Next:
- Optionally add top-level `~/.factory/sessions/*.jsonl` fallback matching by embedded `cwd` if Factory stores sessions outside project-encoded directories in future.

Open questions (UNCONFIRMED if needed):
- UNCONFIRMED: Whether all Amp versions emit `env.initial.trees[].uri`; current implementation falls back to most recent unused thread per live process.

Working set (files/ids/commands):
- `src-tauri/src/agent/{amp.rs,codex.rs,claude_style.rs,process.rs}`
- `src-tauri/src/agent/mod.rs`
- `src-tauri/src/session/{model.rs,parser.rs,mod.rs}`
- `src-tauri/src/tests/session_tests.rs`
- `src/types/session.ts`, `src/components/SessionCard.tsx`, `src/App.tsx`
- `README.md`, `package-lock.json`
- Verification: `cargo test`; `npm test -- --run`; `npm run build`
- Packaging: `npm run tauri -- build`; generated DMG script rerun with `--skip-jenkins`; smoke test launched `/Applications/Agent Sessions 0.2.1.app`
- Makefile verification: `make help`; `make -n dmg`; `make -n install-dmg`; `make dmg`
