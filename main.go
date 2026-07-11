package main

import (
	"math/big"
	"os"
	"path/filepath"
	"time"
	"encoding/json"
	"net/http"
	"log"
)

const epochUnix = 1782086400.0

var (
	epochStartNs *big.Int
	origCwd      string
)

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

func init() {
	log.Println("loading finals.all")
	loadFinals(filepath.Join(itsDir(), "finals.all"))
	buildSpline()

	log.Println("reading offset.dat + nightfall date")
	origCwd, _ = os.Getwd()
	if err := os.Chdir(itsDir()); err != nil {
		log.Fatal(err)
	}
	offset := getOffset()
	ey, em, ed, _ := computeEarliestNight()
	nightMJD := jdn(ey, em, ed) - 2400000.5
	nightDUT1 := interpolateDUT1Spline(nightMJD)

	epochStartNs = new(big.Int).SetInt64(int64(epochUnix))
	epochStartNs.Mul(epochStartNs, big.NewInt(1e9))
	epochStartNs.Add(epochStartNs, big.NewInt(int64(nightDUT1*1e9)))
	epochStartNs.Add(epochStartNs, big.NewInt(int64(offset*1e9)))

	log.Println("ready")
}

func calculateNsecs() *big.Int {
	now := time.Now()
	nowDUT1 := interpolateDUT1Spline(mjdFromUnix(now.Unix()))

	nowNs := new(big.Int).SetInt64(now.Unix())
	nowNs.Mul(nowNs, big.NewInt(1e9))
	nowNs.Add(nowNs, big.NewInt(int64(now.Nanosecond())))
	nowNs.Add(nowNs, big.NewInt(int64(nowDUT1*1e9)))

	return new(big.Int).Sub(nowNs, epochStartNs)
}

func TimeGooner(w http.ResponseWriter, r *http.Request) {
	nsecs := calculateNsecs()
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Access-Control-Allow-Origin", "*")
	json.NewEncoder(w).Encode(map[string]string{
		"nsecs": nsecs.String(),
	})
}

func main() {
	http.HandleFunc("/api/its_nsecs", TimeGooner)
	http.Handle("/", http.FileServer(http.Dir(filepath.Join(origCwd, "static"))))

	port := "127.0.0.1:8080"

	log.Printf("   API: http://127.0.0.1:8080/api/its_nsecs")
	log.Printf("   Static: http://127.0.0.1:8080/")

	if err := http.ListenAndServe(port, nil); err != nil {
		log.Fatal("server error:", err)
	}
}
