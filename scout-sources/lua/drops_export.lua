-- Exports where items come from, according to the Forever Guide addon, as CSV on stdout.
-- Usage : lua drops_export.lua <AddOns/ForeverGuide> <sources | npcs | names>
--
-- sources : kind,item_id,source_id,chance,quest_only,more
--   kind       : npc | skinning | pickpocket | object | container | quest | quest_choice
--   source_id  : NPC, world object (chest, vein…), container item or quest; 0 = "and `more` others" row
--   chance     : percent per kill / opening, empty when unknown (Questie / Forever lists, containers, quests)
--   quest_only : 1 when it only drops while the quest is active
-- npcs    : npc_id,name,min_level,max_level,rank,zone,x,y,dungeon,ui_map   (NPCs used by `sources` only)
-- names   : kind,id,name   (kind object | quest)
-- Names in French (Forever Guide's DataFrFR.lua, from QuestieDB), English otherwise.
local dir, mode = arg[1], arg[2]
assert(dir and mode, "usage : lua drops_export.lua <dossier ForeverGuide> <sources | npcs | names>")

local function field(v)
    if v == nil then return "" end
    v = tostring(v)
    if v:find('[,"\n]') then v = '"' .. v:gsub('"', '""') .. '"' end
    return v
end

-- Forever Guide files start with `local _, ns = ...`. Optional files may be missing.
local ns = {}
local function load(name, required)
    local chunk, err = loadfile(dir .. "/" .. name)
    if not chunk then
        assert(not required, err)
        return
    end
    chunk("ForeverGuide", ns)
end
GetLocale = function() return "frFR" end   -- read by Locale.lua: zone and dungeon names in French
load("ItemSearchData.lua", true)
load("DataFrFR.lua")
load("Locale.lua")
load("DungeonLoot.lua")

local FR = ns.LZ_LOAD and ns.LZ_LOAD.frFR and ns.LZ_LOAD.frFR() or {}

local function split(s)
    local t = {}
    for part in (s .. "\t"):gmatch("([^\t]*)\t") do t[#t + 1] = part end
    return t
end

local function name(kind, id, en, ru)
    local fr = FR[kind] and FR[kind][id]
    if fr and fr ~= "" then return fr end
    if en and en ~= "" then return en end
    return ru ~= "" and ru or nil
end

-- Forever Guide keys → our kinds; `fd` / `qd` / `qo` give no chance.
local KINDS = { d = "npc", fd = "npc", qd = "npc", sk = "skinning", pp = "pickpocket", o = "object",
    qo = "object", ci = "container" }

-- sources[item][kind .. ":" .. id] = { kind, id, chance, quest_only }, more[item][kind] = n
local sources, more = {}, {}
local function add(item, kind, id, chance, questOnly)
    local list = sources[item]
    if not list then
        list = {}
        sources[item] = list
    end
    local key = kind .. ":" .. id
    local old = list[key]
    -- A known chance wins over an unknown one (Questie lists repeat the classic database).
    if old and old.chance and not chance then return end
    list[key] = { kind = kind, id = id, chance = chance, questOnly = questOnly }
end

for item, s in pairs(ns.SI_ITEM or {}) do
    local f = split(s)
    for key, body in (f[4] or ""):gmatch("(%a+)=([^;]*)") do
        for tok in body:gmatch("[^,]+") do
            local n = tok:match("^%+(%d+)$")
            if n then
                local kind = key == "q" and "quest" or KINDS[key]
                if kind then
                    more[item] = more[item] or {}
                    more[item][kind] = (more[item][kind] or 0) + tonumber(n)
                end
            elseif key == "q" then
                local quest, choice = tok:match("^(%d+)(c?)$")
                if quest then add(item, choice == "c" and "quest_choice" or "quest", tonumber(quest)) end
            elseif key == "fd" or key == "qd" or key == "qo" or key == "ci" then
                local id = tonumber(tok, 36)
                if id then add(item, KINDS[key], id) end
            elseif KINDS[key] then
                local id, pct, q = tok:match("^(%w+):([%d%.]+)(q?)$")
                if id then add(item, KINDS[key], tonumber(id, 36), tonumber(pct), q == "q") end
            end
        end
    end
end

-- Dungeon bosses: Forever Guide's own loot tables (newer, Forever dungeons included) override the classic chance.
local bosses = {}   -- npc → { name, level, dungeon key } for bosses missing from SI_NPC
for key, list in pairs(ns.LOOT or {}) do
    for _, boss in ipairs(list) do
        if type(boss.id) == "number" then
            bosses[boss.id] = { name = boss.n, level = tonumber(boss.lv), dungeon = key }
            for _, it in ipairs(boss.items or {}) do
                if type(it.id) == "number" then
                    local list2 = sources[it.id] or {}
                    sources[it.id] = list2
                    list2["npc:" .. boss.id] = { kind = "npc", id = boss.id, chance = tonumber(it.p) }
                end
            end
        end
    end
end

local function sortedKeys(t)
    local keys = {}
    for k in pairs(t) do keys[#keys + 1] = k end
    table.sort(keys, function(a, b)
        if type(a) == type(b) then return a < b end
        return tostring(a) < tostring(b)
    end)
    return keys
end

local NPC_KINDS = { npc = true, skinning = true, pickpocket = true }
local rows = 0

if mode == "sources" then
    io.write("kind,item_id,source_id,chance,quest_only,more\n")
    for _, item in ipairs(sortedKeys(sources)) do
        local list = sources[item]
        for _, key in ipairs(sortedKeys(list)) do
            local s = list[key]
            io.write(table.concat({ s.kind, item, s.id, field(s.chance and string.format("%.4g", s.chance)),
                s.questOnly and "1" or "0", "0" }, ","), "\n")
            rows = rows + 1
        end
        for kind, n in pairs(more[item] or {}) do
            io.write(table.concat({ kind, item, 0, "", "0", n }, ","), "\n")
            rows = rows + 1
        end
    end
elseif mode == "npcs" then
    local used = {}
    for _, list in pairs(sources) do
        for _, s in pairs(list) do
            if NPC_KINDS[s.kind] then used[s.id] = true end
        end
    end
    io.write("npc_id,name,min_level,max_level,rank,zone,x,y,dungeon,ui_map\n")
    for _, npc in ipairs(sortedKeys(used)) do
        local s = ns.SI_NPC and ns.SI_NPC[npc]
        local boss = bosses[npc]
        if s then
            local f = split(s)
            local map = tonumber(f[6])
            local zone = map and map ~= 0 and ns.ZoneLocal and ns.ZoneLocal(map) or nil
            local dungeon = f[9] ~= "" and (ns.InstName and ns.InstName(f[9]) or f[9]) or nil
            if not dungeon and boss and ns.DungeonName then dungeon = ns.DungeonName({ key = boss.dungeon }) end
            io.write(table.concat({ npc, field(name("n", npc, f[2], f[1])), field(tonumber(f[3])),
                field(tonumber(f[4])), field(tonumber(f[5])), field(zone), field(tonumber(f[7])),
                field(tonumber(f[8])), field(dungeon ~= "" and dungeon or nil), field(map ~= 0 and map or nil) }, ","), "\n")
            rows = rows + 1
        elseif boss then
            local dungeon = ns.DungeonName and ns.DungeonName({ key = boss.dungeon }) or nil
            io.write(table.concat({ npc, field(name("n", npc, boss.name, "")), field(boss.level),
                field(boss.level), "1", "", "", "", field(dungeon ~= "" and dungeon or nil), "" }, ","), "\n")
            rows = rows + 1
        end
    end
elseif mode == "names" then
    local used = { object = {}, quest = {} }
    for _, list in pairs(sources) do
        for _, s in pairs(list) do
            if s.kind == "object" then used.object[s.id] = true end
            if s.kind == "quest" or s.kind == "quest_choice" then used.quest[s.id] = true end
        end
    end
    io.write("kind,id,name\n")
    for _, id in ipairs(sortedKeys(used.object)) do
        local s = ns.SI_OBJ and ns.SI_OBJ[id]
        local f = s and split(s) or {}
        local n = name("o", id, f[2], f[1] or "")
        if n then
            io.write("object,", id, ",", field(n), "\n")
            rows = rows + 1
        end
    end
    for _, id in ipairs(sortedKeys(used.quest)) do
        local s = ns.SI_QUEST and ns.SI_QUEST[id]
        local f = s and split(s) or {}
        local n = name("q", id, f[2], f[1] or "")
        if n then
            io.write("quest,", id, ",", field(n), "\n")
            rows = rows + 1
        end
    end
else
    error("mode inconnu : " .. mode)
end

io.stderr:write(rows .. " lignes (" .. mode .. ")\n")
