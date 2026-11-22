# isolock-plan

Airlock dependency resolution planner, lockfile diffing, CI hermeticity drift detection, and GitHub Actions dispatch command generation for Isolock.

`format_dispatch_commands` groups missing package pins by the workflow's registry choices and emits deterministic `gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=… -f packages="…"` commands. These commands require an already reviewed and committed artifact manifest with exact source/filename/SHA-256 pins; they do not resolve registry metadata or dependencies. Package values are constrained to shell-safe characters, and the CLI integration contract checks the generated fields against `.github/workflows/isolock.yml`.
