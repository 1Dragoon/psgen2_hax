##### Character Sheet Struct -- total 84 bytes each character
Field offset -- Meaning
0x00 -- Character ID (0 == Eusis, 1 == Nei, 2 == Rudger, 3 == Anne, 4 == Huey, 5 == Amia, 6 == Keinz, 7 == Silka NB: You can actually change this field in-place. Example: Set Eusis' field to 1 and you've got two Neis)
0x01 -- Playable character (bool) (Whether they're available to add to party at home, this is unset when nei dies, or left intact if she's saved. All other characters start at zero and set to one after you unlock them.)
0x02 -- Poisoned (bool)
0x03 -- Current Level
0x04 -- Current HP
0x05 -- Max HP
0x06 -- Current TP
0x07 -- Max TP
0x08 -- Attack
0x09 -- Defense
0x0A -- Stamina
0x0B -- Intellect
0x0C -- Agility
0x0D -- Luck
0x0E -- Skill
0x0F -- ??
0x10 -- ??
0x11 -- ??
0x12 -- ??
0x13 -- ??
0x14 -- ??
0x15 -- ??
0x16 -- ??
0x17 -- ??
0x18 -- Current XP
0x19 - 0x1E -- Techniques list (24-byte static array, see below for values)
0x1F -- Creating (bool) (if they're at home, 1 is creating, 0 is training)
0x20 -- Number of battles for item creation progress at home

01 Feuer
02 Gifeuer
03 Nafeuer
04 Water
05 Giwater
06 Gisawater
07 Zonde
08 Gizonde
09 Gisazonde
0A Zan
0B Gizan
0C Nazan
0D Gravito
0E Gigravito
0F Nagravito
10 Glanz
11 Giglanz
12 Naglanz
13 Shifter
14 Vampir
15 Ager
16 Prozedun
17 Konter
18 Seizures
19 Gadge
1A Gigadge
1B Nagadge
1C Sagadge
1D Gisagadge
1E Nasagadge
1F Genera
20 Sagenera
21 Volt
22 Savolt
23 Schutz
24 Saschutz
25 Drunk
26 Limiter
27 Shinparo
28 Falser
29 Limit
2A D-Wand
2B Schneller
2C Saschneller
2D Rester
2E Girester
2F Narester
30 Sarester
31 Gisarester
32 Nasarester
33 Sacra
34 Nasacra
35 Anti
36 Reverser
37 Ruckkehr
38 Hinaus
39 Musik
3A Megiddo

##### These are apparently zero cost techniques, but they're not normally obtainable as such. Likely just a "spell effect" of various items and special skills. You can, for example, allow Amia to cast Sulfur by dropping 0x4a into her technique array.
3B Beguiling ocarina
3C Monofluid
3D Difluid
3E Trifluid
3F Monomate
40 Dimate
41 Trimate
42 Antidote
43 Moon Atomizer
44 Star Atomizer
45 Sol Atomizer
46 Royal Guard
47 Final Force
48 Revenge
49 Nareverser
4A Sulfur
4B Medical Treatment
4C Explosive Touch
4D Tornado
4E Sleep
4F Jellen
50 Zalure
51 Drunk
52 Doku


#### Item Creation
- Anne, Huey, and Silka can create healing items by themselves (yes, every single healing item.) I didn't take time to figure out exact values, but I plugged in 255 for each, and all yielded sol atomizers.
- Amia can make cakes by herself (all of them.) 255 yielded Naula cake.
- Keinz and Rudger can only make antidotes. At least, I tried 255, that's what I got. Theoretical maximum is exactly 2^30, or roughly one billion. Any potential values beyond this is an exercise for the reader, just as are any lower values.
- "Leaving everybody at home" can yield any item you want, but it introduces a lot of randomness. The same applies with triple groups yielding what a component double group does. Example: I plugged in 59 battles each 2for Anne, Silka, Amia, and Keinz. Depending on how long I walked around outside of Eusis' house, it would yield either white boots or naula cake. My hunch is that the more people you add, the fewer battles needed, but the more random the outcome. I don't know the exact amounts as I haven't really experimented with it.
- There doesn't appear to be any kind of "creation mode". Once the characters are set to create, all that matters is the battle count. Simply plugging in high numbers lets you literally walk out and right back in and, bam, item is created. If their battle counts are too low, you get nothing but the numbers do not reset either. In fact, swapping out somebody while their counts are low preserves those numbers even while they're out and about with you. Possibly exploitable? *shrug*
- Eusis and Nei (yes, you can leave them at home if you finagle their "playable character" bit) can be set to create and have it flag as such in memory, and they can accumulate battle counts, but they don't appear to go towards anything. They don't even appear to speed up creation times even though their numbers get consumed when an item is created by others.