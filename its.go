package main

/*
#cgo LDFLAGS: -lm
#include <stdlib.h>
#include "common.h"
*/
import "C"
import (
	"unsafe"
)

func loadFinals(path string) {
	cp := C.CString(path)
	C.load_finals(cp)
	C.free(unsafe.Pointer(cp))
}

func buildSpline() {
	C.build_spline()
}

func interpolateDUT1Spline(mjd float64) float64 {
	return float64(C.interpolate_dut1_spline(C.double(mjd)))
}

func mjdFromUnix(t int64) float64 {
	return float64(C.mjd_from_unix(C.time_t(t)))
}

func freeEOP() {
	C.free_eop()
}


