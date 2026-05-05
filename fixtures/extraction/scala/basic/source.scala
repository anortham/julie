package fixture

trait Job {
  def run(): Int
}

class Worker(val id: Int) extends Job {
  def run(): Int = helper(id)

  private def helper(value: Int): Int = value + 1
}
