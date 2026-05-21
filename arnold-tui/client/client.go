package client

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"sync"

	"github.com/jbankse/arnold/arnold-tui/wire"
)

// Client is a connection to arnoldd over UDS. Send writes one request frame
// per call (newline-delimited JSON); Events returns a read-only channel that
// produces decoded DaemonEvents until the connection closes.
type Client struct {
	conn   net.Conn
	w      *bufio.Writer
	events <-chan wire.DaemonEvent
	errs   <-chan error
	closed chan struct{}
	once   sync.Once
}

func socketPath() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(home, ".arnold", "arnold.sock"), nil
}

// Dial connects to the daemon. Returns an error if the socket doesn't exist
// (typically: daemon not running) — caller can show "is arnoldd running?" UX.
func Dial(ctx context.Context) (*Client, error) {
	path, err := socketPath()
	if err != nil {
		return nil, err
	}
	d := net.Dialer{}
	conn, err := d.DialContext(ctx, "unix", path)
	if err != nil {
		return nil, fmt.Errorf("cannot connect to %s (%w); is arnoldd running?", path, err)
	}

	events := make(chan wire.DaemonEvent, 32)
	errs := make(chan error, 1)
	closed := make(chan struct{})

	c := &Client{
		conn:   conn,
		w:      bufio.NewWriter(conn),
		events: events,
		errs:   errs,
		closed: closed,
	}

	go c.readLoop(events, errs, closed)
	return c, nil
}

// Send writes one request frame followed by '\n'.
func (c *Client) Send(req wire.ClientRequest) error {
	b, err := req.Encode()
	if err != nil {
		return err
	}
	if _, err := c.w.Write(b); err != nil {
		return err
	}
	if err := c.w.WriteByte('\n'); err != nil {
		return err
	}
	return c.w.Flush()
}

// Events returns the channel events arrive on. Closes when the daemon hangs up.
func (c *Client) Events() <-chan wire.DaemonEvent { return c.events }

// Errs returns a one-shot channel that fires if the read loop crashed.
func (c *Client) Errs() <-chan error { return c.errs }

// Close half-closes the write side and waits for read to drain.
func (c *Client) Close() error {
	c.once.Do(func() {
		_ = c.conn.Close()
		<-c.closed
	})
	return nil
}

func (c *Client) readLoop(events chan<- wire.DaemonEvent, errs chan<- error, closed chan<- struct{}) {
	defer close(events)
	defer close(closed)
	dec := bufio.NewScanner(c.conn)
	dec.Buffer(make([]byte, 0, 1024*1024), 16*1024*1024) // up to 16MB per line for large memory bodies
	for dec.Scan() {
		line := dec.Bytes()
		if len(line) == 0 {
			continue
		}
		var ev wire.DaemonEvent
		if err := json.Unmarshal(line, &ev); err != nil {
			// Drop malformed lines; signal but don't crash.
			select {
			case errs <- fmt.Errorf("malformed daemon frame: %w", err):
			default:
			}
			continue
		}
		events <- ev
	}
	if err := dec.Err(); err != nil && !errors.Is(err, net.ErrClosed) {
		select {
		case errs <- err:
		default:
		}
	}
}
