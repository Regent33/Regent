#!/usr/bin/env bun
// Entry point: hand the argv to the command router and exit with its code.
import { runCli } from "@app/cli/router.ts";

const code = await runCli(process.argv.slice(2));
process.exit(code);
