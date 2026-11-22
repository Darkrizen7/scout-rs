-- Exports the past auction house prices kept by Auctionator and Forever Guide, as CSV on stdout.
-- Usage : lua legacy_export.lua <dossier SavedVariables>
--
-- Colonnes : source,item_id,timestamp,price,quantity
--   auctionator  : un relevé par jour (prix le plus bas vu ce jour-là, quantité max), daté de midi
--   foreverguide : le dernier relevé de chaque objet, à l'heure exacte
--   quantity peut être vide (Auctionator ne l'a pas toujours).
local dir = assert(arg[1], "usage : lua legacy_export.lua <dossier SavedVariables>")

-- Minimal CBOR decoder (what C_EncodingUtil.SerializeCBOR produces: ints, floats, strings, arrays, maps).
local cbor = {}

function cbor.decode(s)
    local pos = 1
    local item
    local function arg(info)
        if info < 24 then return info end
        local size = ({ [24] = 1, [25] = 2, [26] = 4, [27] = 8 })[info]
        assert(size, "CBOR : longueur indéfinie non gérée")
        local v = string.unpack(">I" .. size, s, pos)
        pos = pos + size
        return v
    end
    function item()
        local b = s:byte(pos)
        pos = pos + 1
        local major, info = b >> 5, b & 31
        if major == 0 then return arg(info)
        elseif major == 1 then return -1 - arg(info)
        elseif major == 2 or major == 3 then
            local n = arg(info)
            local v = s:sub(pos, pos + n - 1)
            pos = pos + n
            return v
        elseif major == 4 then
            local t = {}
            for i = 1, arg(info) do t[i] = item() end
            return t
        elseif major == 5 then
            local t = {}
            for _ = 1, arg(info) do
                local k = item()
                t[k] = item()
            end
            return t
        elseif major == 6 then arg(info) return item()   -- tag: ignored
        elseif major == 7 then
            if info == 20 then return false elseif info == 21 then return true
            elseif info == 22 or info == 23 then return nil
            elseif info == 25 then error("CBOR : demi-flottant non géré")
            elseif info == 26 then local v = string.unpack(">f", s, pos) pos = pos + 4 return v
            elseif info == 27 then local v = string.unpack(">d", s, pos) pos = pos + 8 return v
            end
        end
        error(("CBOR : octet 0x%02x inattendu en position %d"):format(b, pos - 1))
    end
    local v = item()
    assert(pos == #s + 1, "CBOR : données en trop")
    return v
end


local function load(name)
    local env = {}
    local chunk = loadfile(dir .. "/" .. name .. ".lua", "t", env)
    if not chunk then return nil end
    chunk()
    return env
end

local rows = 0
local function emit(source, id, t, price, quantity)
    io.write(source, ",", id, ",", t, ",", price or "", ",", quantity or "", "\n")
    rows = rows + 1
end

io.write("source,item_id,timestamp,price,quantity\n")

-- Auctionator: AUCTIONATOR_PRICE_DATABASE[realm] = CBOR string or table of
-- [itemKey] = { m = current price, l/h = lowest/highest minimum per day, a = max available per day }.
-- Days count from 2020-01-01 local midnight (Auctionator.Constants.SCAN_DAY_0).
local auctionator = load("Auctionator")
local DAY_0 = os.time({ year = 2020, month = 1, day = 1, hour = 0 })
for realm, data in pairs(auctionator and auctionator.AUCTIONATOR_PRICE_DATABASE or {}) do
    if type(data) == "string" then data = cbor.decode(data) end
    if type(data) == "table" then
        for key, item in pairs(data) do
            local id = tonumber(key)   -- skips gear/pet keys ("g:…", "p:…")
            if id and type(item) == "table" and item.h then
                for day, high in pairs(item.h) do
                    local price = item.l and item.l[day] or high
                    local available = item.a and item.a[day]
                    emit("auctionator", id, DAY_0 + tonumber(day) * 86400 + 12 * 3600, price, available)
                end
            end
        end
    end
end

-- Forever Guide: ForeverGuideDB.ah[realm-side].items[id] = { m = unit minimum, q = quantity, t = time seen }.
local guide = load("ForeverGuide")
local ah = guide and guide.ForeverGuideDB and guide.ForeverGuideDB.ah or {}
for _, store in pairs(ah) do
    if type(store) == "table" and type(store.items) == "table" then
        for id, e in pairs(store.items) do
            if type(id) == "number" and e.m and e.t then emit("foreverguide", id, e.t, e.m, e.q) end
        end
    end
end

io.stderr:write(rows .. " lignes\n")
