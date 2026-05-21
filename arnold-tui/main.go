package main

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/jbankse/arnold/arnold-tui/wire"
)

func main() {
	// Smoke test: decode a known good frame.
	sample := `{"type":"reply","session_id":"00000000-0000-0000-0000-000000000000","text":"hi"}`
	ev, err := wire.DecodeEvent([]byte(sample))
	if err != nil {
		fmt.Fprintf(os.Stderr, "decode failed: %v\n", err)
		os.Exit(1)
	}
	out, _ := json.MarshalIndent(ev, "", "  ")
	fmt.Println(string(out))
}
