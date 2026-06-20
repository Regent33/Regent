//go:build !windows

package ui

// EnableVT is a no-op outside Windows — Unix terminals speak ANSI natively.
func EnableVT() {}
