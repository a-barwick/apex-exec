# Apex Exec for VS Code

This thin client starts the `apex-exec lsp` and `apex-exec dap` stdio servers.
It provides inline diagnostics, go-to-definition, references, rename, coverage
requests, breakpoints, stepping, stack frames, variables, database inspection,
and transaction timelines.

For local development:

```bash
npm ci
npm test
code .
```

Press `F5` from this directory to launch an Extension Development Host, or
package the directory with `npx @vscode/vsce package`. Set
`apexExec.executable` when the binary is not on `PATH`.

Project debugging requires a launch configuration with the SFDX project
directory in `program` and a public static zero-argument `Class.method` in
`target`. Script debugging uses the active `.apex` file as `program`.

The committed lockfile and CI `npm ci` step make dependency installation
reproducible. The current `npm test` command is deliberately a scoped syntax
smoke test for this thin client. Behavioral activation, request, Unicode
position, URI, and malformed-protocol tests remain owned by S2-03 after the
structured diagnostic prerequisite; the smoke test is not presented as that
future coverage.
