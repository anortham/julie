class_name Worker
extends Node

signal activated(value)

var id: int

func _init(value: int) -> void:
    id = value

func run() -> int:
    return helper(id)

func helper(value: int) -> int:
    return value + 1
