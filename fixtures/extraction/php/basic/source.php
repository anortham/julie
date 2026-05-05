<?php

namespace Fixture;

class Worker
{
    public int $id;

    public function __construct(int $id)
    {
        $this->id = $id;
    }

    public function run(): int
    {
        return helper($this->id);
    }
}

function helper(int $value): int
{
    return $value + 1;
}
