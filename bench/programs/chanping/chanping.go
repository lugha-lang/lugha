package main

import (
	"fmt"
	"os"
	"strconv"
)

func main() {
	n, _ := strconv.ParseInt(os.Args[1], 10, 64)
	ping := make(chan int64)
	pong := make(chan int64)
	done := make(chan struct{})
	go func() {
		for v := range ping {
			pong <- v + 1
		}
		close(done)
	}()
	var v int64
	for i := int64(0); i < n; i++ {
		ping <- v
		v = <-pong
	}
	close(ping)
	<-done
	fmt.Println(v)
}
