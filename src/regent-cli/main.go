// regent — the CLI plane (ADR-012). All state and behavior live in the
// Rust daemon; this binary is a thin JSON-RPC client.
package main

import "regent/cli/app"

func main() {
	app.Execute()
}
