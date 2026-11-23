-- Wraps an exporter run inside the tool: what it writes to stdout is collected and returned,
-- stderr is dropped, and loadfile reads through Rust (Unicode paths on Windows).
local read_file = ...

local out, n = {}, 0
local function write(self, ...)
    for i = 1, select("#", ...) do
        local v = select(i, ...)
        n = n + 1
        -- Same text as the real io.write: floats as "%.14g" (tostring would add ".0").
        out[n] = math.type(v) == "float" and string.format("%.14g", v) or tostring(v)
    end
    return self
end
local function noop(self) return self end

io.stdout = { write = write, flush = noop, close = noop }
io.stderr = { write = noop, flush = noop, close = noop }
io.write = function(...) return write(io.stdout, ...) end

function loadfile(path, mode, env)
    local text, err = read_file(path)
    if not text then return nil, err end
    -- An explicit nil env would leave the chunk without globals.
    if env == nil then return load(text, "@" .. path, mode) end
    return load(text, "@" .. path, mode, env)
end

return function(chunk, args)
    arg = table.move(args, 1, #args, 1, { [0] = "export" })
    chunk(table.unpack(args))
    return table.concat(out)
end
