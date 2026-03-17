defmodule MyApp.Calculator do
  @moduledoc "A simple calculator"

  @doc "Adds two numbers"
  def add(a, b), do: a + b

  defp validate(n) when is_number(n), do: n
  defp validate(_), do: raise("not a number")

  def multiply(a, b) do
    validate(a)
    validate(b)
    a * b
  end
end

defprotocol Printable do
  @doc "Prints to string"
  def to_string(data)
end

defimpl Printable, for: Integer do
  def to_string(n), do: Integer.to_string(n)
end

defmodule MyApp.User do
  defstruct [:name, :email, :age]

  defmacro validate_field(field) do
    quote do: is_binary(unquote(field))
  end
end

defmodule MyApp.Service do
  use GenServer
  import Enum, only: [map: 2, filter: 2]
  alias MyApp.{User, Calculator}
  require Logger

  @type state :: %{count: integer()}
  @callback init(args :: term()) :: {:ok, state()}

  @spec start_link(keyword()) :: GenServer.on_start()
  def start_link(opts), do: GenServer.start_link(__MODULE__, opts)
end
