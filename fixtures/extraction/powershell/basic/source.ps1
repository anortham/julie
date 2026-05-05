function Invoke-Helper {
    param([int]$Value)
    return $Value + 1
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
