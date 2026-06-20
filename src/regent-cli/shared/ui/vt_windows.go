//go:build windows

package ui

import (
	"os"
	"syscall"
	"unsafe"
)

// EnableVT switches the Windows console into VT/ANSI mode so the palette
// renders in classic conhost too (Windows Terminal already supports it).
func EnableVT() {
	const enableVirtualTerminalProcessing = 0x0004
	kernel32 := syscall.NewLazyDLL("kernel32.dll")
	getConsoleMode := kernel32.NewProc("GetConsoleMode")
	setConsoleMode := kernel32.NewProc("SetConsoleMode")

	handle := syscall.Handle(os.Stdout.Fd())
	var mode uint32
	r, _, _ := getConsoleMode.Call(uintptr(handle), uintptr(unsafe.Pointer(&mode)))
	if r == 0 {
		return // not a console (piped) — nothing to enable
	}
	setConsoleMode.Call(uintptr(handle), uintptr(mode|enableVirtualTerminalProcessing))
}
