package main

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"syscall"
	"time"

	"github.com/jbankse/arnold/arnold-tui/client"
	"github.com/jbankse/arnold/arnold-tui/tui"
)

func main() {
	ctx := context.Background()

	// Try once. If the socket isn't there (no daemon), spawn one detached and
	// poll until it answers.
	cli, err := client.Dial(ctx)
	if err != nil {
		if spawnErr := ensureDaemon(); spawnErr != nil {
			fmt.Fprintf(os.Stderr, "could not start arnoldd: %v\n", spawnErr)
			fmt.Fprintf(os.Stderr, "original dial error: %v\n", err)
			os.Exit(1)
		}
		cli, err = waitForDaemon(ctx, 10*time.Second)
		if err != nil {
			fmt.Fprintf(os.Stderr, "daemon spawned but did not become reachable: %v\n", err)
			os.Exit(1)
		}
	}
	defer cli.Close()

	m := tui.New(cli)
	if err := m.Run(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "tui error: %v\n", err)
		os.Exit(1)
	}
}

// ensureDaemon forks arnoldd in the background, detached from this process so
// it survives TUI exit. Logs to ~/.arnold/arnoldd.log / .err. No-op if the
// binary is already running and bound to the socket — caller checks that first.
func ensureDaemon() error {
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	arnoldDir := filepath.Join(home, ".arnold")
	if err := os.MkdirAll(arnoldDir, 0755); err != nil {
		return err
	}

	// Locate arnoldd: same directory as the TUI binary, or fall back to PATH.
	exe, err := os.Executable()
	if err != nil {
		return err
	}
	binPath := filepath.Join(filepath.Dir(exe), "arnoldd")
	if _, statErr := os.Stat(binPath); statErr != nil {
		// Fall back to PATH lookup.
		alt, lookErr := exec.LookPath("arnoldd")
		if lookErr != nil {
			return fmt.Errorf("arnoldd not found alongside %s or on PATH: %w", exe, lookErr)
		}
		binPath = alt
	}

	logF, err := os.OpenFile(filepath.Join(arnoldDir, "arnoldd.log"), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return err
	}
	errF, err := os.OpenFile(filepath.Join(arnoldDir, "arnoldd.err"), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		logF.Close()
		return err
	}

	cmd := exec.Command(binPath)
	cmd.Stdout = logF
	cmd.Stderr = errF
	// Detach: new session so the child doesn't die when arnold (the TUI) exits.
	cmd.SysProcAttr = &syscall.SysProcAttr{Setsid: true}

	if err := cmd.Start(); err != nil {
		logF.Close()
		errF.Close()
		return err
	}
	// Release the child; we don't reap it. logF/errF stay open in the child's fds.
	_ = cmd.Process.Release()
	return nil
}

// waitForDaemon polls Dial every 200ms up to `timeout` and returns the first
// successful Client. Used right after ensureDaemon to wait for the socket bind.
func waitForDaemon(ctx context.Context, timeout time.Duration) (*client.Client, error) {
	deadline := time.Now().Add(timeout)
	var lastErr error
	for time.Now().Before(deadline) {
		cli, err := client.Dial(ctx)
		if err == nil {
			return cli, nil
		}
		lastErr = err
		time.Sleep(200 * time.Millisecond)
	}
	return nil, fmt.Errorf("daemon socket not reachable within %s: %w", timeout, lastErr)
}
