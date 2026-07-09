all: build

build:
	go build -o itsite

run: build
	./itsite

clean:
	rm -f itsite

.PHONY: all build run clean
