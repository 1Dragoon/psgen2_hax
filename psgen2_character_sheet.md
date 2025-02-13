# Character Sheet Notes

These are some nuggets I've found about character tracking while trying to figure out various things in PSGEN2

##### Character Sheet Struct -- total 84 bytes each character

Field offset -- Meaning
- 0x00 -- Character ID (0 == Eusis, 1 == Nei, 2 == Rudger, 3 == Anne, 4 == Huey, 5 == Amia, 6 == Keinz, 7 == Silka NB: You can actually change this field in-place. Example: Set Eusis' field to 1 and you've got two Neis)
- 0x01 -- Playable character (bool) (Whether they're available to add to party at home, this is unset when nei dies, or left intact if she's saved. All other characters start at zero and set to one after you unlock them.)
- 0x02 -- Poisoned (bool)
- 0x03 -- Current Level
- 0x04 -- Current HP
- 0x05 -- Max HP
- 0x06 -- Current TP
- 0x07 -- Max TP
- 0x08 -- Attack
- 0x09 -- Defense
- 0x0A -- Stamina
- 0x0B -- Intellect
- 0x0C -- Agility
- 0x0D -- Luck
- 0x0E -- Skill
- 0x0F -- ??
- 0x10 -- ??
- 0x11 -- ??
- 0x12 -- ??
- 0x13 -- ??
- 0x14 -- ??
- 0x15 -- ??
- 0x16 -- ??
- 0x17 -- ??
- 0x18 -- Current XP (set to e.g. 9,999,999 to instantly max level this character after one fight)
- 0x19 - 0x1E -- Techniques list (24-byte static array, see below for values)
- 0x1F -- Creating (bool) (if they're at home, 1 is creating, 0 is training)
- 0x20 -- Number of battles for item creation progress at home

As for the values with unknown purposes, I have a few hunches what they might do. One for example might determine what their special skill is, another might determine how long it takes for their skill to progress. I haven't yet experimented with those numbers.

#### Technique Values
- 0x01 == Feuer
- 0x02 == Gifeuer
- 0x03 == Nafeuer
- 0x04 == Water
- 0x05 == Giwater
- 0x06 == Gisawater
- 0x07 == Zonde
- 0x08 == Gizonde
- 0x09 == Gisazonde
- 0x0A == Zan
- 0x0B == Gizan
- 0x0C == Nazan
- 0x0D == Gravito
- 0x0E == Gigravito
- 0x0F == Nagravito
- 0x10 == Glanz
- 0x11 == Giglanz
- 0x12 == Naglanz
- 0x13 == Shifter
- 0x14 == Vampir
- 0x15 == Ager
- 0x16 == Prozedun
- 0x17 == Konter
- 0x18 == Seizures
- 0x19 == Gadge
- 0x1A == Gigadge
- 0x1B == Nagadge
- 0x1C == Sagadge
- 0x1D == Gisagadge
- 0x1E == Nasagadge
- 0x1F == Genera
- 0x20 == Sagenera
- 0x21 == Volt
- 0x22 == Savolt
- 0x23 == Schutz
- 0x24 == Saschutz
- 0x25 == Drunk
- 0x26 == Limiter
- 0x27 == Shinparo
- 0x28 == Falser
- 0x29 == Limit
- 0x2A == D-Wand
- 0x2B == Schneller
- 0x2C == Saschneller
- 0x2D == Rester
- 0x2E == Girester
- 0x2F == Narester
- 0x30 == Sarester
- 0x31 == Gisarester
- 0x32 == Nasarester
- 0x33 == Sacra
- 0x34 == Nasacra
- 0x35 == Anti
- 0x36 == Reverser
- 0x37 == Ruckkehr
- 0x38 == Hinaus
- 0x39 == Musik
- 0x3A == Megiddo

##### These are apparently zero cost techniques, but they're not normally obtainable as such. Likely just a "spell effect" of various items and special skills. You can, for example, allow Amia to cast Sulfur by dropping 0x4a into her technique array.
- 0x3B == Beguiling ocarina
- 0x3C == Monofluid
- 0x3D == Difluid
- 0x3E == Trifluid
- 0x3F == Monomate
- 0x40 == Dimate
- 0x41 == Trimate
- 0x42 == Antidote
- 0x43 == Moon Atomizer
- 0x44 == Star Atomizer
- 0x45 == Sol Atomizer
- 0x46 == Royal Guard
- 0x47 == Final Force
- 0x48 == Revenge
- 0x49 == Nareverser
- 0x4A == Sulfur
- 0x4B == Medical Treatment
- 0x4C == Explosive Touch
- 0x4D == Tornado
- 0x4E == Sleep
- 0x4F == Jellen
- 0x50 == Zalure
- 0x51 == Drunk
- 0x52 == Doku


#### Item Creation
- All characters have their battles tracked for item creation in the very last field of the character sheet struct.
- Anne, Huey, and Silka can create healing items by themselves (yes, every single healing item.) I didn't take time to figure out exact values, but I plugged in 255 for each, and all yielded sol atomizers.
- Amia can make cakes by herself (all of them.) 255 yielded Naula cake.
- Keinz and Rudger can only make antidotes. At least, I tried 255, that's what I got. Theoretical maximum is exactly 2^30, or roughly one billion. Any potential values beyond this is an exercise for the reader, just as are any lower values.
- "Leaving everybody at home" can yield any item you want, but it introduces a lot of randomness. The same applies with triple groups yielding what a component double group does. Example: I plugged in 59 battles each 2for Anne, Silka, Amia, and Keinz. Depending on how long I walked around outside of Eusis' house, it would yield either white boots or naula cake. My hunch is that the more people you add, the fewer battles needed, but the more random the outcome. I don't know the exact amounts as I haven't really experimented with it.
- There doesn't appear to be any kind of "creation mode". Once the characters are set to create, all that matters is the battle count. Simply plugging in high numbers lets you literally walk out and right back in and, bam, item is created. If their battle counts are too low, you get nothing but the numbers do not reset either. In fact, swapping out somebody while their counts are low preserves those numbers even while they're out and about with you. Possibly exploitable? *shrug*
- Eusis and Nei (yes, you can leave them at home if you finagle their "playable character" bit) can be set to create and have it flag as such in memory, and they can accumulate battle counts, but they don't appear to go towards anything. They don't even appear to speed up creation times even though their numbers get consumed when an item is created by others.