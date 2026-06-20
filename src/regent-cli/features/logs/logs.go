// Package logs wires `regent logs` — view the daemon's redacted rolling log
// file under $REGENT_HOME/logs/.
package logs

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"time"

	"github.com/spf13/cobra"
)

// Command builds `regent logs`. home resolves the active profile's REGENT_HOME.
func Command(home func() string) *cobra.Command {
	var follow bool
	cmd := &cobra.Command{
		Use:   "logs",
		Short: "Show the daemon log (newest rolling file)",
		Args:  cobra.NoArgs,
		RunE: func(_ *cobra.Command, _ []string) error {
			path, err := latestLog(filepath.Join(home(), "logs"))
			if err != nil {
				return err
			}
			return tail(path, follow)
		},
	}
	cmd.Flags().BoolVarP(&follow, "follow", "f", false, "stream new log lines as they arrive")
	return cmd
}

// latestLog returns the newest regent.log* file in dir. The daily appender's
// date suffix sorts chronologically, so the last match is current.
func latestLog(dir string) (string, error) {
	matches, err := filepath.Glob(filepath.Join(dir, "regent.log*"))
	if err != nil {
		return "", err
	}
	if len(matches) == 0 {
		return "", fmt.Errorf("no log files in %s (has the daemon run yet?)", dir)
	}
	sort.Strings(matches)
	return matches[len(matches)-1], nil
}

// tail prints the file; when follow, keeps copying appended bytes until the
// process is interrupted (Ctrl-C).
func tail(path string, follow bool) error {
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()
	for {
		// io.Copy stops at EOF (returning nil); the file offset persists, so
		// the next pass picks up bytes appended since.
		if _, err := io.Copy(os.Stdout, f); err != nil {
			return err
		}
		if !follow {
			return nil
		}
		time.Sleep(500 * time.Millisecond)
	}
}
