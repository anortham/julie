defmodule Fixture.Worker do
  @spec run(integer()) :: integer()
  def run(id) do
    helper(id)
  end

  defp helper(value) do
    value + 1
  end
end
