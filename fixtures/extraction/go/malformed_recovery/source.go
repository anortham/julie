package main

type Empty struct{}

type EmbeddedStruct struct {
    Empty
    value int
}

type MissingBrace struct {
    field int

func VariadicFunction(format string, args ...interface{}) {
    fmt.Printf(format, args...)
}
