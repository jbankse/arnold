package main

import (
	"context"
	"fmt"
	"os"

	"github.com/jbankse/arnold/arnold-tui/client"
	"github.com/jbankse/arnold/arnold-tui/tui"
)

func main() {
	ctx := context.Background()
	cli, err := client.Dial(ctx)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
	defer cli.Close()

	m := tui.New(cli)
	if err := m.Run(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "tui error: %v\n", err)
		os.Exit(1)
	}
}
