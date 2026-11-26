-- Exports the skill level of the recipe items (patterns) Scout saw in game, as CSV on stdout.
-- Usage: lua pattern_export.lua <SavedVariables/Scout.lua>
--
-- Columns: item_id,skill (ScoutDB.craft.patterns[itemID].req); items and levels only.
-- A pattern without a numeric level is skipped.
local scoutPath = arg[1]
assert(scoutPath, "usage : lua pattern_export.lua <Scout.lua>")

io.write("item_id,skill\n")
local rows = 0

local env = {}
local chunk = loadfile(scoutPath, "t", env)
if chunk then
    chunk()
    local patterns = ((env.ScoutDB or {}).craft or {}).patterns or {}
    for id, p in pairs(patterns) do
        if type(id) == "number" and type(p) == "table" and type(p.req) == "number" then
            io.write(string.format("%d,%d\n", id, p.req))
            rows = rows + 1
        end
    end
end

io.stderr:write(rows .. " niveaux de patrons\n")
