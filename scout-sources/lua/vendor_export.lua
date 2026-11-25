-- Exports the items merchants (NPCs) sell and their unit price, as CSV on stdout.
-- Usage : lua vendor_export.lua <SavedVariables/Scout.lua>
--
-- Colonnes : source,item_id,price,npc_id,npc,zone,x,y,limited
--   scout        : vu chez un marchand en jeu par Scout, avec le PNJ (ScoutDB.merchants) quand il est connu,
--                  sinon le seul prix (ScoutDB.vendor, relevés plus anciens) ; prix unitaire, remise comprise
-- price en cuivre, éventuellement fractionnaire (prix d'un lot divisé par sa taille) ;
-- x, y en pourcentage sur la carte de la zone ; limited = 1 pour un stock limité.
local scoutPath = arg[1]
assert(scoutPath, "usage : lua vendor_export.lua <Scout.lua>")

local function field(v)
    if v == nil then return "" end
    v = tostring(v)
    if v:find('[,"\n]') then v = '"' .. v:gsub('"', '""') .. '"' end
    return v
end

io.write("source,item_id,price,npc_id,npc,zone,x,y,limited\n")
local rows = 0
local function emit(source, id, price, npcID, m, limited)
    if type(id) ~= "number" or type(price) ~= "number" or price <= 0 then return end
    m = m or {}
    io.write(table.concat({ source, id, string.format("%.4g", price), field(npcID), field(m.name),
        field(m.zone), field(m.x), field(m.y), limited and "1" or "0" }, ","), "\n")
    rows = rows + 1
end

local env = {}
local chunk = loadfile(scoutPath, "t", env)
if chunk then
    chunk()
    local db = env.ScoutDB or {}
    local withNpc = {}
    for npcID, m in pairs(db.merchants or {}) do
        for id, e in pairs(m.items or {}) do
            emit("scout", id, e.p, npcID, m, e.l)
            withNpc[id] = true
        end
    end
    -- Prices seen before merchants were recorded: no NPC known.
    for id, price in pairs(db.vendor or {}) do
        if not withNpc[id] then emit("scout", id, price) end
    end
end

io.stderr:write(rows .. " prix marchands\n")
