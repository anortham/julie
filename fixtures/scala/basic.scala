package com.example

import scala.collection.mutable.ListBuffer

sealed trait Animal {
  def speak(): String
}

case class Dog(name: String) extends Animal {
  override def speak(): String = s"Woof! I'm $name"
}

object DogFactory {
  def create(name: String): Dog = Dog(name)
}

abstract class Shape(val sides: Int) {
  def area(): Double
}

val pi: Double = 3.14159
var count: Int = 0
type StringList = List[String]
