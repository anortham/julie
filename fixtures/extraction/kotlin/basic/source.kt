package fixture

interface Job {
    fun run(): Int
}

class Worker(private val id: Int) : Job {
    override fun run(): Int {
        return helper(id)
    }

    private fun helper(value: Int): Int {
        return value + 1
    }
}
