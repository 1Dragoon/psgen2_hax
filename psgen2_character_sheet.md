# Character Sheet Notes

These are some nuggets I've found about character tracking while trying to figure out various things in PSGEN2

#### Character Sheet Struct -- total 84 bytes each character

Eusis's character sheet struct begins first at 0x2476F4 for his character ID

Field offset -- Meaning
0x00 -- Character id (0 == Eusis, 1 == Nei, 2 == Rudger, 3 == Anne, 4 == Huey, 5 == Amia, 6 == Keinz, 7 == Silka NB: You can actually change this field in-place. Example: Set Eusis' field to 1 and you've got two Neis)
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

##### Technique values:

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

##### These are apparently zero cost techniques, but they're not normally obtainable as actual techniques. Likely just a "spell effect" of various items and special skills. You can, for example, allow Amia to cast Sulfur by dropping 0x4a into her technique array.
3B Beguiling ocarin
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


#### Inventory and Items: 0x247B14 (location of first inventory slot)

There appear to be a total of 128 inventory slots and 128 storage slots. Each slot is 4 bytes, it looks like the first byte indicates the item ID number, the second byte appears to be an enchantment modifier, with the third doing a mix of indicating enchantment and where it's equipped, and fourth simply indicating whether it's worn on the legs.

Second byte:
0x10 == Lightning enchant
0x20 == Air enchant
0x40 == ?? (I've seen this one set in the wild, not sure what it does)
0x80 == ??
0x04 == Fire enchant
0x08 == Ice enchant

Third byte, left nibble:
0x0 == Equipped on head
0x4 == Equipped on torso
0x8 == Equipped on right hand
0xC == Equipped on left hand

Other values in the left nibble seem to be treated as a higher order value for who has it. 3F seems to indicate nobody has it equipped.

Third byte, right nibble is the character ID its equipped to. E.g.
0x41 == Equipped on Nei's torso
0xC1 == Equipped on Nei's left hand
0x00 == Equipped on Eusis's head
0xB2 == Equipped on Rudger's right hand

Fourth byte:
0x01 == Legs
All other bit fields appear like they might be ignored, but uncertain.

3F appears to mean nobody is holding or equipping it, it's just in your inventory

##### Complete item ID list:

01 Knife
02 Ceramic Knife
03 Laser Knife
04 Silver Knife
05 Dagger
06 Ceramic Dagger
07 Wind Dagger
08 Laconian Dagger
09 Scalpel
0A Laser Scalpel
0B Sword
0C Ceramic Sword
0D Flame Sword
0E Laser Sword
0F Angry Sword
10 Plasma Sword
11 Laconian Sword
12 Chain Sword
13 Nei Sword
14 Silent Shot
15 Poison Shot
16 Acid Shot
17 Laser Shot
18 Napalm Shot
19 Nei Shot
1A Sonic Gun
1B Wave Gun
1C Shotgun
1D Vulcan
1E Laser Vulcan
1F Pulse Vulcan
20 Cannon
21 Laser Cannon
22 Pulse Cannon
23 Plasma Cannon
24 Needle Gun
25 Boomerang
26 Slicer
27 Titanium Slicer
28 Laser Slicer
29 Fire Slicer
2A Ice Slicer
2B Laconian Slicer
2C Nei Slicer
2D Whip
2E Lightning Whip
2F Blazing Cane
30 Freezing Cane
31 Windblade Cane
32 Steel Scale
33 Ceramic Scale
34 Laconian Scale
35 Steel Claw
36 Saber Claw
37 Ceramic Claw
38 Laser Claw
39 Silent Claw
3A Animal Claw
3B Headgear
3C Fiberglass Gear
3D Titanium Gear
3E Crescent Gear
3F Storm Gear
40 Ceramic Gear
41 Laconian Gear
42 Ribbon
43 Silver Ribbon
44 Jewel Ribbon
45 Silver Crown
46 Jewel Crown
47 Snow Crown
48 Thunder Crown
49 Nei Crown
4A Titanium Helmet
4B Laconian Helmet
4C Nei Helmet
4D Wind Bandanna
4E Rainbow Bandanna
4F Gale Bandanna
50 Magic Hat
51 Mogic Hat
52 Carbon Suit
53 Carbon Vest
54 Fiberglass Vest
55 Fiberglass Coat
56 Fiberglass Mantle
57 White Mantle
58 Titanium Chestplate
59 Ceramic Chestplate
5A Zirconium Chestplate
5B Crystal Chestplate
5C Laconian Chestplate
5D Titanium Armor
5E Ceramic Armor
5F Zirconium Armor
60 Fibrillae
61 Titanium Fibrillae
62 Ceramic Fibrillae
63 Laconian Fibrillae
64 Titanium Harnisch
65 Crystal Harnisch
66 Laconian Harnisch
67 Nei Harnisch
68 Crystal Field
69 Plasma Field
6A Nei Field
6B Amber Mantle
6C Prison Uniform
6D Carbon Shield
6E Fiberglass Shield
6F Titanium Shield
70 Mirror Shield
71 Ceramic Shield
72 Tranquil Shield
73 Laser Shield
74 Laconian Shield
75 Nei Shield
76 Carbon Armel
77 Fiberglass Armel
78 Titanium Armel
79 Mirror Armel
7A Ceramic Armel
7B Luminous Armel
7C Laconian Armel
7D Nei Armel
7E Green Sleeve
7F Sincere Sleeve
80 Leather Shoes
81 Espadrilles
82 Leather Boots
83 Knife Boots
84 Long Boots
85 Heilsam Boots
86 Schneller Boots
87 Black Boots
88 White Boots
89 Guard Boots
8A Monomate
8B Dimate
8C Trimate
8D Monofluid
8E Difluid
8F Trifluid
90 Antidote        
91 Moon Atomizer
92 Star Atomizer
93 Sol Atomizer
94 Traveling Ocarina
95 Hesitant Ocarina
96 Covert Ocarina
97 Beguiling Ocarina
98 Shortcake
99 Mont Blanc Cake
9A Fruit Cake
9B Chiffon Cake
9C Naula Style Cake
9D Land Master
9E Jet Scooter
9F Ice Digger
A0 Dynamite
A1 Atlas
A2 Ransom Note
A3 System Recorder
A4 Maruera Leaves
A5 Maruera Gum
A6 Aeroprism
A7 Plasma Ring
A8 Heal Ring
A9 Golden Stone
AA Enhancer
AB Main Key
AC Digital Music Score
AD Motavian Ring
AE Maruera Berries
AF Happiness Ring
B0 Visiphone
B1 Red Card
B2 Yellow Card
B3 Blue Card
B4 Green Card
B5 Container Key
B6 Tube Key
B7 Polymetryl
B8 Silver Bullet Necklace
B9 Nanette's Letter

##### Hex string I generated to dump into the inventory table to see the entire list of items

01 00 3F 00 02 00 3F 00 03 00 3F 00 04 00 3F 00 05 00 3F 00 06 00 3F 00 07 00 3F 00 08 00 3F 00 09 00 3F 00 0A 00 3F 00 0B 00 3F 00 0C 00 3F 00 0D 00 3F 00 0E 00 3F 00 0F 00 3F 00 10 00 3F 00 11 00 3F 00 12 00 3F 00 13 00 3F 00 14 00 3F 00 15 00 3F 00 16 00 3F 00 17 00 3F 00 18 00 3F 00 19 00 3F 00 1A 00 3F 00 1B 00 3F 00 1C 00 3F 00 1D 00 3F 00 1E 00 3F 00 1F 00 3F 00 20 00 3F 00 21 00 3F 00 22 00 3F 00 23 00 3F 00 24 00 3F 00 25 00 3F 00 26 00 3F 00 27 00 3F 00 28 00 3F 00 29 00 3F 00 2A 00 3F 00 2B 00 3F 00 2C 00 3F 00 2D 00 3F 00 2E 00 3F 00 2F 00 3F 00 30 00 3F 00 31 00 3F 00 32 00 3F 00 33 00 3F 00 34 00 3F 00 35 00 3F 00 36 00 3F 00 37 00 3F 00 38 00 3F 00 39 00 3F 00 3A 00 3F 00 3B 00 3F 00 3C 00 3F 00 3D 00 3F 00 3E 00 3F 00 3F 00 3F 00 40 00 3F 00 41 00 3F 00 42 00 3F 00 43 00 3F 00 44 00 3F 00 45 00 3F 00 46 00 3F 00 47 00 3F 00 48 00 3F 00 49 00 3F 00 4A 00 3F 00 4B 00 3F 00 4C 00 3F 00 4D 00 3F 00 4E 00 3F 00 4F 00 3F 00 50 00 3F 00 51 00 3F 00 52 00 3F 00 53 00 3F 00 54 00 3F 00 55 00 3F 00 56 00 3F 00 57 00 3F 00 58 00 3F 00 59 00 3F 00 5A 00 3F 00 5B 00 3F 00 5C 00 3F 00 5D 00 3F 00 5E 00 3F 00 5F 00 3F 00 60 00 3F 00 61 00 3F 00 62 00 3F 00 63 00 3F 00 64 00 3F 00 65 00 3F 00 66 00 3F 00 67 00 3F 00 68 00 3F 00 69 00 3F 00 6A 00 3F 00 6B 00 3F 00 6C 00 3F 00 6D 00 3F 00 6E 00 3F 00 6F 00 3F 00 70 00 3F 00 71 00 3F 00 72 00 3F 00 73 00 3F 00 74 00 3F 00 75 00 3F 00 76 00 3F 00 77 00 3F 00 78 00 3F 00 79 00 3F 00 7A 00 3F 00 7B 00 3F 00 7C 00 3F 00 7D 00 3F 00 7E 00 3F 00 7F 00 3F 00 80 00 3F 00 81 00 3F 00 82 00 3F 00 83 00 3F 00 84 00 3F 00 85 00 3F 00 86 00 3F 00 87 00 3F 00 88 00 3F 00 89 00 3F 00 8A 00 3F 00 8B 00 3F 00 8C 00 3F 00 8D 00 3F 00 8E 00 3F 00 8F 00 3F 00 90 00 3F 00 91 00 3F 00 92 00 3F 00 93 00 3F 00 94 00 3F 00 95 00 3F 00 96 00 3F 00 97 00 3F 00 98 00 3F 00 99 00 3F 00 9A 00 3F 00 9B 00 3F 00 9C 00 3F 00 9D 00 3F 00 9E 00 3F 00 9F 00 3F 00 A0 00 3F 00 A1 00 3F 00 A2 00 3F 00 A3 00 3F 00 A4 00 3F 00 A5 00 3F 00 A6 00 3F 00 A7 00 3F 00 A8 00 3F 00 A9 00 3F 00 AA 00 3F 00 AB 00 3F 00 AC 00 3F 00 AD 00 3F 00 AE 00 3F 00 AF 00 3F 00 B0 00 3F 00 B1 00 3F 00 B2 00 3F 00 B3 00 3F 00 B4 00 3F 00 B5 00 3F 00 B6 00 3F 00 B7 00 3F 00 B8 00 3F 00 B9 00 3F 00 BA 00 3F 00 BB 00 3F 00 BC 00 3F 00 BD 00 3F 00 BE 00 3F 00 BF 00 3F 00 C0 00 3F 00 C1 00 3F 00 C2 00 3F 00 C3 00 3F 00 C4 00 3F 00 C5 00 3F 00 C6 00 3F 00 C7 00 3F 00 C8 00 3F 00 C9 00 3F 00 CA 00 3F 00 CB 00 3F 00 CC 00 3F 00 CD 00 3F 00 CE 00 3F 00 CF 00 3F 00 D0 00 3F 00 D1 00 3F 00 D2 00 3F 00 D3 00 3F 00 D4 00 3F 00 D5 00 3F 00 D6 00 3F 00 D7 00 3F 00 D8 00 3F 00 D9 00 3F 00 DA 00 3F 00 DB 00 3F 00 DC 00 3F 00 DD 00 3F 00 DE 00 3F 00 DF 00 3F 00 E0 00 3F 00 E1 00 3F 00 E2 00 3F 00 E3 00 3F 00 E4 00 3F 00 E5 00 3F 00 E6 00 3F 00 E7 00 3F 00 E8 00 3F 00 E9 00 3F 00 EA 00 3F 00 EB 00 3F 00 EC 00 3F 00 ED 00 3F 00 EE 00 3F 00 EF 00 3F 00 F0 00 3F 00 F1 00 3F 00 F2 00 3F 00 F3 00 3F 00 F4 00 3F 00 F5 00 3F 00 F6 00 3F 00 F7 00 3F 00 F8 00 3F 00 F9 00 3F 00 FA 00 3F 00 FB 00 3F 00 FC 00 3F 00 FD 00 3F 00 FE 00 3F 00 FF 00 3F 00

Note that everything above 89 is some kind of invalid item.

#### Item Creation
- Anne, Huey, and Silka can create healing items by themselves (yes, every single healing item.) I didn't take time to figure out exact values, but I plugged in 255 for each, and all yielded sol atomizers.
- Amia can make cakes by herself (all of them.) 255 yielded Naula cake.
- Keinz and Rudger can only make antidotes. At least, I tried 255, that's what I got. Theoretical maximum is exactly 2^30, or roughly one billion. Any potential values beyond 255 is an exercise for the reader, just as are any lower values.
- "Leaving everybody at home" can yield any item you want, but it introduces a lot of randomness. The same applies with triple groups yielding what a component double group does. Example: I plugged in 59 battles each 2for Anne, Silka, Amia, and Keinz. Depending on how long I walked around outside of Eusis' house, it would yield either white boots or naula cake. My hunch is that the more people you add, the fewer battles needed, but the more random the outcome. I don't know the exact amounts as I haven't really experimented with it.
- There doesn't appear to be any kind of "creation mode". Once the characters are set to create, all that matters is the battle count. Simply plugging in high numbers lets you literally walk out and right back in and, bam, item is created. If their battle counts are too low, you get nothing but the numbers do not reset either. In fact, swapping out somebody while their counts are low preserves those numbers even while they're out and about with you. Possibly exploitable? *shrug*
- Eusis and Nei (yes, you can leave them at home if you finagle their "playable character" bit) can be set to create and have it flag as such in memory, and they can accumulate battle counts, but they don't appear to go towards anything. They don't even appear to speed up creation times even though their numbers get consumed when an item is created by others.
