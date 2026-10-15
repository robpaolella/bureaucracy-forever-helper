BureaucracyForeverDB = {
	["activeRaid"] = "cm9xr2k4a0001",
	["activeBy"] = "chosen",
	["chosenBy"] = "Rae Quill",
	["selectionAt"] = 1792001000,
	["selectionBy"] = "Rae Quill",
	["selectionCode"] = "MC-0114",
	["ui"] = {
		["point"] = {
			"CENTER", -- [1]
			"UIParent", -- [2]
			"CENTER", -- [3]
			-12.5, -- [4]
			40, -- [5]
		},
		["minimap"] = false,
	},
	["raids"] = {
		["cm9xr2k4a0001"] = {
			["name"] = "Molten Core",
			["instance"] = 409,
		},
	},
	["awards"] = {
		["cm9xr2k4a0001"] = {
			{
				["key"] = "aw-1111aaaa",
				["raid"] = "cm9xr2k4a0001",
				["env"] = "staging",
				["boss"] = "lb_lucifron",
				["item"] = 16805,
				["char"] = "Ana Vale",
				["method"] = "HR",
				["roll"] = 91,
				["sender"] = "Rae Quill",
				["at"] = 1792001200,
				["state"] = "confirmed",
				["stateAt"] = 1792001203,
				["voided"] = true,
				["voidReason"] = "Wrong \"player\"\nsee chat",
				["voidedAt"] = 1792001300,
			}, -- [1]
		},
	},
	["ops"] = {
		["cm9xr2k4a0001"] = {
			{
				["type"] = "award",
				["id"] = "aw-1111aaaa",
				["formatVersion"] = 1,
				["raid"] = "cm9xr2k4a0001",
				["env"] = "staging",
				["boss"] = "lb_lucifron",
				["item"] = 16805,
				["char"] = "Ana Vale",
				["method"] = "HR",
				["roll"] = 91,
				["sender"] = "Rae Quill",
				["at"] = 1792001200,
			}, -- [1]
			{
				["type"] = "confirm",
				["id"] = "cf-2222bbbb",
				["formatVersion"] = 1,
				["raid"] = "cm9xr2k4a0001",
				["env"] = "staging",
				["key"] = "aw-1111aaaa",
				["at"] = 1792001203,
			}, -- [2]
			{
				["type"] = "void",
				["id"] = "vd-3333cccc",
				["formatVersion"] = 1,
				["raid"] = "cm9xr2k4a0001",
				["env"] = "staging",
				["key"] = "aw-1111aaaa",
				["reason"] = "Wrong \"player\"\nsee chat",
				["at"] = 1792001300,
			}, -- [3]
		},
	},
	["noRaid"] = {
		{
			["key"] = "aw-4444dddd",
			["boss"] = "lb_magmadar",
			["item"] = 17069,
			["char"] = "Ana Vale",
			["method"] = "OPEN_ROLL",
			["sender"] = "Rae Quill",
			["at"] = 1792002000,
			["state"] = "announced",
			["stateAt"] = 1792002000,
		}, -- [1]
	},
	["noRaidOps"] = {
		{
			["type"] = "award",
			["id"] = "aw-4444dddd",
			["formatVersion"] = 1,
			["boss"] = "lb_magmadar",
			["item"] = 17069,
			["char"] = "Ana Vale",
			["method"] = "OPEN_ROLL",
			["sender"] = "Rae Quill",
			["at"] = 1792002000,
		}, -- [1]
	},
}
