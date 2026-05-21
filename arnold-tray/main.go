package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"time"

	"github.com/getlantern/systray"
)

func main() {
	systray.Run(onReady, onExit)
}

func onReady() {
	systray.SetIcon(iconYellow)
	systray.SetTitle("")
	systray.SetTooltip("Arnold — connecting…")

	mOpen := systray.AddMenuItem("Open arnold", "Launch the arnold TUI in a new Terminal window")
	systray.AddSeparator()
	mRestart := systray.AddMenuItem("Restart daemon", "Stop and relaunch arnoldd")
	systray.AddSeparator()
	mQuit := systray.AddMenuItem("Quit arnold-tray", "Exit the menu bar app")

	// Ping loop
	go pingLoop()

	for {
		select {
		case <-mOpen.ClickedCh:
			openArnoldTUI()
		case <-mRestart.ClickedCh:
			restartDaemon()
		case <-mQuit.ClickedCh:
			systray.Quit()
			return
		}
	}
}

func onExit() {}

func pingLoop() {
	for {
		if pingDaemon(2 * time.Second) {
			systray.SetIcon(iconGreen)
			systray.SetTooltip("Arnold — daemon running")
		} else {
			systray.SetIcon(iconRed)
			systray.SetTooltip("Arnold — daemon not responding")
		}
		time.Sleep(5 * time.Second)
	}
}

func pingDaemon(timeout time.Duration) bool {
	home, err := os.UserHomeDir()
	if err != nil {
		return false
	}
	sock := filepath.Join(home, ".arnold", "arnold.sock")
	ctx, cancel := context.WithTimeout(context.Background(), timeout)
	defer cancel()
	d := net.Dialer{}
	conn, err := d.DialContext(ctx, "unix", sock)
	if err != nil {
		return false
	}
	defer conn.Close()
	// Send a Ping frame and expect Pong.
	if _, err := conn.Write([]byte("{\"type\":\"ping\"}\n")); err != nil {
		return false
	}
	conn.SetReadDeadline(time.Now().Add(timeout))
	buf := make([]byte, 1024)
	n, err := conn.Read(buf)
	if err != nil || n == 0 {
		return false
	}
	var ev map[string]any
	if err := json.Unmarshal(buf[:n], &ev); err != nil {
		// Maybe got more than one line; try the first line only.
		for i, b := range buf[:n] {
			if b == '\n' {
				_ = json.Unmarshal(buf[:i], &ev)
				break
			}
		}
	}
	return ev["type"] == "pong"
}

func openArnoldTUI() {
	// Use osascript to open a new Terminal window running `arnold`.
	// AppleScript: tell app "Terminal" to do script "arnold"
	script := `tell application "Terminal" to do script "arnold"`
	cmd := exec.Command("osascript", "-e", script)
	if err := cmd.Run(); err != nil {
		fmt.Fprintln(os.Stderr, "open arnold failed:", err)
	}
}

func restartDaemon() {
	// Use launchctl to unload + reload the LaunchAgent.
	home, _ := os.UserHomeDir()
	plist := filepath.Join(home, "Library", "LaunchAgents", "com.arnold.arnoldd.plist")
	_ = exec.Command("launchctl", "unload", plist).Run()
	_ = exec.Command("launchctl", "load", plist).Run()
}
