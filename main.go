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

func redirectHTTP(w http.ResponseWriter, r *http.Request) {
	http.Redirect(w, r, "https://its.impostorboy.ru"+r.URL.Path, http.StatusMovedPermanently)
}

func main() {
	http.HandleFunc("/api/its_nsecs", TimeGooner)
	http.Handle("/", http.FileServer(http.Dir("./static")))

	port := ":443"
	certFile := "/etc/letsencrypt/live/its.impostorboy.ru/fullchain.pem"
	keyFile := "/etc/letsencrypt/live/its.impostorboy.ru/privkey.pem"

	log.Printf("   API: https://its.impostorboy.ru/api/its_nsecs")
	log.Printf("   Static: https://its.impostorboy.ru/")

	go func() {
		mux := http.NewServeMux()
		mux.HandleFunc("/", redirectHTTP)
		if err := http.ListenAndServe(":80", mux); err != nil {
			log.Fatal("HTTP redirect server error:", err)
		}
	}()

	if err := http.ListenAndServeTLS(port, certFile, keyFile, nil); err != nil {
		log.Fatal("HTTPS server error:", err)
	}
}
