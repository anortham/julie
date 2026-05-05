class Worker
  def initialize(id)
    @id = id
  end

  def run
    helper(@id)
  end

  private

  def helper(value)
    value + 1
  end
end
