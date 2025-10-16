use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coroutines_and_async_patterns() {
        let code = r#"
-- Basic coroutine
local function worker()
  local count = 0

  while true do
    local input = coroutine.yield("Working... " .. count)
    count = count + 1

    if input == "stop" then
      break
    end
  end

  return "Worker finished"
end

-- Create coroutine
local workerCo = coroutine.create(worker)

-- Producer-consumer pattern
local function producer()
  local data = {"item1", "item2", "item3", "item4"}

  for i = 1, #data do
    local item = data[i]
    coroutine.yield(item)
  end

  return "Producer done"
end

local function consumer()
  local producerCo = coroutine.create(producer)
  local results = {}

  while coroutine.status(producerCo) ~= "dead" do
    local success, value = coroutine.resume(producerCo)

    if success and value then
      local processed = "Processed: " .. value
      table.insert(results, processed)
      print(processed)
    end
  end

  return results
end

-- Async-like pattern with callbacks
local function asyncOperation(data, callback)
  local timer = {
    delay = 1000,
    callback = callback,
    data = data
  }

  local function complete()
    local result = "Completed: " .. timer.data
    timer.callback(nil, result)
  end

  complete()
end

-- Promise-like pattern
local Promise = {}
Promise.__index = Promise

function Promise:new(executor)
  local instance = setmetatable({}, Promise)
  instance.state = "pending"
  instance.value = nil
  instance.handlers = {}

  local function resolve(value)
    if instance.state == "pending" then
      instance.state = "fulfilled"
      instance.value = value
      instance:_runHandlers()
    end
  end

  local function reject(reason)
    if instance.state == "pending" then
      instance.state = "rejected"
      instance.value = reason
      instance:_runHandlers()
    end
  end

  executor(resolve, reject)
  return instance
end

function Promise:then(onFulfilled, onRejected)
  local newPromise = Promise:new(function(resolve, reject)
    local handler = {
      onFulfilled = onFulfilled,
      onRejected = onRejected,
      resolve = resolve,
      reject = reject
    }

    if self.state == "pending" then
      table.insert(self.handlers, handler)
    else
      self:_handleHandler(handler)
    end
  end)

  return newPromise
end

function Promise:_runHandlers()
  for _, handler in ipairs(self.handlers) do
    self:_handleHandler(handler)
  end
  self.handlers = {}
end

function Promise:_handleHandler(handler)
  if self.state == "fulfilled" then
    if handler.onFulfilled then
      local success, result = pcall(handler.onFulfilled, self.value)
      if success then
        handler.resolve(result)
      else
        handler.reject(result)
      end
    else
      handler.resolve(self.value)
    end
  elseif self.state == "rejected" then
    if handler.onRejected then
      local success, result = pcall(handler.onRejected, self.value)
      if success then
        handler.resolve(result)
      else
        handler.reject(result)
      end
    else
      handler.reject(self.value)
    end
  end
end

local function async(fn)
  return function(...)
    local args = {...}
    local co = coroutine.create(fn)

    local function step(success, err, value)
      if not success then
        error(err)
      end

      local state = coroutine.status(co)
      if state == "dead" then
        return
      end

      local yielded = err
      if type(yielded) == "table" and yielded.then then
        yielded:then(
          function(result)
            step(coroutine.resume(co, nil, result))
          end,
          function(error)
            step(false, error)
          end
        )
      else
        step(coroutine.resume(co, nil, yielded))
      end
    end

    step(coroutine.resume(co, table.unpack(args)))
  end
end

local function await(promise)
  return coroutine.yield(promise)
end

local fetchData = async(function(url)
  local data = await(Promise:new(function(resolve, _reject)
    local response = {
      status = 200,
      body = "Response from " .. url
    }
    resolve(response)
  end))

  return data
end)

local function range(start, stop)
  return coroutine.create(function()
    for i = start, stop do
      coroutine.yield(i)
    end
  end)
end

local function map(iter, fn)
  return coroutine.create(function()
    while coroutine.status(iter) ~= "dead" do
      local success, value = coroutine.resume(iter)
      if success and value then
        coroutine.yield(fn(value))
      end
    end
  end)
end

local function filter(iter, predicate)
  return coroutine.create(function()
    while coroutine.status(iter) ~= "dead" do
      local success, value = coroutine.resume(iter)
      if success and value and predicate(value) then
        coroutine.yield(value)
      end
    end
  end)
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "coroutines.lua".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        let worker = symbols.iter().find(|s| s.name == "worker");
        assert!(worker.is_some());
        assert_eq!(worker.unwrap().kind, SymbolKind::Function);

        let worker_co = symbols.iter().find(|s| s.name == "workerCo");
        assert!(worker_co.is_some());
        assert_eq!(worker_co.unwrap().kind, SymbolKind::Variable);

        let producer = symbols.iter().find(|s| s.name == "producer");
        assert!(producer.is_some());

        let consumer = symbols.iter().find(|s| s.name == "consumer");
        assert!(consumer.is_some());

        let async_operation = symbols.iter().find(|s| s.name == "asyncOperation");
        assert!(async_operation.is_some());

        let promise = symbols.iter().find(|s| s.name == "Promise");
        assert!(promise.is_some());
        assert_eq!(promise.unwrap().kind, SymbolKind::Class);

        let promise_new = symbols
            .iter()
            .find(|s| s.name == "new" && s.parent_id == Some(promise.unwrap().id.clone()));
        assert!(promise_new.is_some());
        assert_eq!(promise_new.unwrap().kind, SymbolKind::Method);

        let promise_then = symbols
            .iter()
            .find(|s| s.name == "then" && s.parent_id == Some(promise.unwrap().id.clone()));
        assert!(promise_then.is_some());

        let run_handlers = symbols
            .iter()
            .find(|s| s.name == "_runHandlers" && s.parent_id == Some(promise.unwrap().id.clone()));
        assert!(run_handlers.is_some());
        assert_eq!(run_handlers.unwrap().visibility, Some(Visibility::Private));

        let async_fn = symbols.iter().find(|s| s.name == "async");
        assert!(async_fn.is_some());
        let await_fn = symbols.iter().find(|s| s.name == "await");
        assert!(await_fn.is_some());

        let fetch_data = symbols.iter().find(|s| s.name == "fetchData");
        assert!(fetch_data.is_some());

        let range_generator = symbols.iter().find(|s| s.name == "range");
        assert!(range_generator.is_some());

        let map_fn = symbols.iter().find(|s| s.name == "map");
        assert!(map_fn.is_some());

        let filter_fn = symbols.iter().find(|s| s.name == "filter");
        assert!(filter_fn.is_some());
    }
}
