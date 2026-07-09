package main

import (
	"bufio"
	"math/big"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
	"encoding/json"
	"net/http"
	"log"
)

const epochUnix = 1782086400.0

func itsDir() string {
	if d := os.Getenv("ITS_DIR"); d != "" {
		return d
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "."
	}
	return filepath.Join(home, "its")
}

func readOffset() float64 {
	f, err := os.Open(filepath.Join(itsDir(), "offset.dat"))
	if err != nil {
		panic(err)
	}
	defer f.Close()
	s := bufio.NewScanner(f)
	if !s.Scan() {
		panic("offset.dat: empty")
	}
	v, err := strconv.ParseFloat(strings.TrimSpace(s.Text()), 64)
	if err != nil {
		panic(err)
	}
	return v
}

func calculateNsecs() *big.Int {
	loadFinals(filepath.Join(itsDir(), "finals.all"))
	buildSpline()

	offset := readOffset()

	now := time.Now()
	nowUnix := now.Unix()
	nowNsec := now.Nanosecond()

	epochDUT1 := interpolateDUT1Spline(mjdFromUnix(int64(epochUnix)))
	nowDUT1 := interpolateDUT1Spline(mjdFromUnix(nowUnix))

	epochStartSec := epochUnix + epochDUT1 + offset
	nowUT1Sec := float64(nowUnix) + nowDUT1 + float64(nowNsec)/1e9
	deltaSec := nowUT1Sec - epochStartSec

	freeEOP()

	f := big.NewFloat(deltaSec * 1e9)
	f.Add(f, big.NewFloat(0.5))
	nsecs, _ := f.Int(nil)
	return nsecs
}

func TimeGooner(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Access-Control-Allow-Origin", "*")

	json.NewEncoder(w).Encode(map[string]*big.Int{
		"nsecs": calculateNsecs(),
	})
}

func main() {
	http.HandleFunc("/api/its_nsecs", TimeGooner)
	http.Handle("/", http.FileServer(http.Dir("./static")))

	// HTTPS shit
	port := ":1337"
	certFile := "./certs/server.crt"
	keyFile := "./certs/server.key"

	log.Printf("   API: https://localhost%s/api/its_nsecs", port)
	log.Printf("   Static: https://localhost%s/", port)

	if err := http.ListenAndServeTLS(port, certFile, keyFile, nil); err != nil {
		log.Fatal("HTTPS server error:", err)
	}
}

func init() {
	log.Println("starting")
	log.Println()
}
