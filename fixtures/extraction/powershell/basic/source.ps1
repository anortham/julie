function Invoke-Helper {
    param([int]$Value)
    return $Value + 1
}

function Invoke-Run {
    param([int]$Value)
    return Invoke-Helper $Value
}

class Worker {
    [int]$Id

    Worker([int]$id) {
        $this.Id = $id
    }

    [int] Run() {
        return Invoke-Helper $this.Id
    }
}
