#### Interesting memory locations:

I found some scalar values that appeared to change as the plot progressed in different locations in memory. I didn't have names for them other than to simply label them A B C D E and F. A seemed to just track which chests have been opened, so I relabeled that one. The rest I never felt a reason to rename, but here they are:

Plot counter B appears to track the main plot. Setting all bits to one will result in the consult dialogue talking about Mother Brain being defeated, where setting them to all zeroes starts with the very first consult dialogue. It also tracks a few reversible bits as well, namely who was in the party when beginning basically any given action.

Plot counter C appears to focus more on subplots. It's possible that B and C are connected, though I don't want to assume so given there are 11 bytes worth of zeros between them.

Chests opened tracker: 0x2475C4
- After finishing the game, mine looked like this:
FF FF FF 15 7F FF FF FF FF FF 7F C0 FF FF FF 1D
- Note some of these chests will just flip back off immediately after turning them on; these are usually the garbage chests found in Ruron and/or the newspaper chests found in Skure

Plot counter B byte sequence: 0x2475E4
1. - 0x10 == Anne joins party cutscene already started
   - 0x20 == Huey joins party cutscene already started
   - 0x40 == Amia joins party cutscene already started
   - 0x80 == Keinz joins party cutscene already started
   - 0x08 == Rudger joins party cutscene already started
   - End: 0xF8
2. - 0x10 == Got the tube key from governor
   - 0x20 == Took letter and key from rascals leader. Zeroing it out allows repeating. Requires third byte to be either 0x40 or 0x10 (or both) in order to reach this step.
   - 0x80 == Picked up shuren southwest 1f dynamite (used key)
   - 0x40 == Picked up shuren northwest 1f monomate (used key)
   - 0x01 == Silka joins party cutscene already started
   - 0x04 == System recorder picked up in biolab
   - 0x08 == Handed system recorder to governor
   - End: 0xFD
3. - 0x10 == Spoke to Michael or Esther in Arimaya, triggers "Teim" consult dialogue
   - 0x20 == Spoke to Jake, Raven or Leah in Paseo -- triggers "go to arimaya" consult dialogue
   - 0x40 == Spoke to Peter in Arimaya, triggers "go to shuren" consult dialogue if above is active
   - 0x80 == Got Musik tech from Aventino
   - 0x01 == Picked up shuren northeast 1f 150 meseta (used key)
   - 0x02 == Picked up shuren southeast 1f dynamite (used key)
   - 0x04 == Spoke to Teim, she begins following
   - 0x08 == Darum/Teim cutscene finished. Note: Can't start it until rudger cutscene start bit is active (0x08 of byte 1) and 0x04 of this byte is active
   - End: 0xFF
4. - 0x10 == viewed the system recorder graph at the library
   - 0x20 == spoke to motavian second time after giving him polymetryl
   - 0x40 == boarded jet scooter for first time
   - 0x80 == got maruera leaves from maruera tree
   - 0x01 == nido door blown open
   - 0x02 == biolab 1f door blown open
   - 0x04 == biolab 3f door blown open
   - 0x08 == used tube key on west bridge door
   - End: 0xFF
5. - 0x10 == After receiving silver bullet necklace and dialogue completes, some nei backstory stuff
   - 0x20 == Amia in the party while talking to Motavian in southeast Optano
   - 0x40 == Spoke to Gabrielle in Piata about uzo island after getting Silka and speaking to Lot and Agag
   - 0x80 == Spoke to Shelly (Sherry?) in Piata Control Tower
   - 0x01 == Got maruera gum from Hiram
   - 0x04 == activated after dialog from first Darum approach is finished
   - 0x08 == after getting Teim, consulted and eusis asks where her father is, sent to north bridge. *Does not trigger if 0x04 is set, instead Eusis just talks about going to the north bridge and this value is never set. So this is an "either or" with the above bit in normal gameplay.
   - End: 0xF5
6. - 0x20 == Bought polymetryl from Solomon in Zema
   - 0x40 == Answered no when asked to fight mother brain
   - 0x80 == Gave Uriel a trimate in Zema
   - 0x01 == Dark Force defeated
   - 0x02 == After defeating mother brain, Lutz and Eusis give a speech, player is given back control, then this bit is set
   - 0x04 == Keinz spoke to Janis in Piata about Professor Luveno
   - 0x08 == Spoke to Hiram with Huey in the party after getting Maruera gum
   - End: 0xEF
7. - 0x10 == Picked up green card
   - 0x20 == Picked up blue card
   - 0x40 == Picked up yellow card
   - 0x80 == Picked up red card
   - 0x08 == Set after defeating neifirst
   - End: 0xF8
8. - 0x80 == Spoke to governor without Rudger (must happen before others join)
   - 0x40 == Spoke to governor without Silka (must happen before others join)
   - 0x10 == Opened green dam
   - 0x02 == Opened yellow dam
   - 0x04 == Opened red dam
   - 0x08 == Opened blue dam
   - End: 0xDE
9. - 0x10 == entered shuren 1f
   - 0x20 == entered nido 1f
   - 0x40 == entered north bridge/tunnel (Darum location)
   - 0x80 == entered biolab 1f
   - 0x01 == Spoke to governor without Anne (must happen before others join)
   - 0x02 == Spoke to governor without Huey (must happen before others join)
   - 0x04 == Spoke to governor without Amia (must happen before others join)
   - 0x08 == Spoke to governor without Keinz (must happen before others join)
   - End: 0xFF
10. - 0x10 == entered control tower 1f
    - 0x20 == entered red dam 1f
    - 0x40 == entered yellow dam 1f
    - 0x80 == entered blue dam 1f
    - 0x01 == entered west bridge
    - 0x02 == entered roron dump 1f
    - 0x04 == entered uzo island
    - 0x08 == entered amedas b1f
    - End: 0xFF
12. - 0x10 == talk to people/enter buildings with anne in the party. Unsets if she isn't in the party or dead when you do these things.
    - 0x20 == talk to people/enter buildings with huey in the party. Unsets if she isn't in the party or dead when you do these things.
    - 0x40 == talk to people/enter buildings with amia in the party. Unsets if she isn't in the party or dead when you do these things.
    - 0x80 == talk to people/enter buildings with keinz in the party. Unsets if he isn't in the party or dead when you do these things.
    - 0x01 == entered green dam 1f
    - 0x02 == talk to people/enter buildings with eusis in the party. Unsets if he isn't in the party or dead when you do these things.
    - 0x04 == talk to people/enter buildings with nei in the party. Unsets if she isn't in the party or dead when you do these things.
    - 0x08 == talk to people/enter buildings with rudger in the party. Unsets if he isn't in the party when you do these things.
    - End: 0x4F (if rudger, nei, eusis, amia)
13. - 0x10 == talk to people/enter buildings with silka in the party. Unsets if she isn't in the party when you do these things.
    - 0x20 == entered Piata (does not allow Silka join/teleport, just triggers NPC/consult progress)
    - 0x40 == entered Kueris (does not allow Keinz join/teleport, just triggers NPC/consult progress)
    - 0x80 == entered Zema (does not allow Amia join/teleport, just triggers NPC/consult progress)
    - 0x04 == Begin Eusis dream after gaira satellite
    - End: 0xE4 (without Silka, 0xF4 with silka)
14. - 0x10 == Consult 3 times in a row after Darum/Teim scene, Nei backstory stuff
    - 0x20 == Consult after getting maruera leaves
    - 0x40 == Set after receiving silver bullet necklace and dialogue completes, some neigh backstory stuff in here
    - 0x80 == Set after defeating neifirst
    - 0x01 == governor tells you to go home to meet huey after analyzing system recorder (I suspect it may also trigger if you somehow enter a later town without doing this)
    - 0x02 == entered Optano (does not allow Anne join/teleport, just triggers NPC/consult progress)
    - 0x04 == entered Arimaya (does not allow Rudger join/teleport, just triggers NPC/consult progress)
    - 0x08 == initial value (entered Paseo?)
    - End: 0xFF
15. - 0x01 == Consult after picking up all Nei armaments


Plot counter C byte sequence: 0x2475FD
1. - 0x01 == Spoke to Zachariah in Zema (far east side) once about Motavian ring
   - 0x02 == Spoke to Zachariah in Zema (far east side) twice about Motavian ring
   - 0x04 == Spoke to Carmel in Zema (far east side) twice about Roron
   - 0x08 == Spoke to Deborah in Zema (far east side) about Motavian ring (only after Zachariah)
   - 0x10 == Spoke to Uriel in Zema about maruera berries (only after Zachariah and Enos)
   - 0x20 == Spoke to Luke in Zema about maruera berries (only after Uriel about the same topic)
   - 0x40 == Got happiness ring from Enos in Zema
   - 0x80 == Got Nanettes Letter
   - End: 0xFF
2. - 0x10 == Spoke to roron b3f motavian about polymetryl
   - 0x20 == Bought polymetryl from Solomon in Zema (toggles off if you give it to the wrong motavian)
   - 0x40 == Polymetryl given to correct motavian in roron
   - 0x80 == Spoke to Abel in Piata about space accident 10 years ago
   - 0x01 == Got maruera berries from Enos
   - 0x02 == Said no to Hiram in Kueris when asked about maruera leaves
   - 0x04 == Got motavian ring from south Kueris Motavian
   - 0x08 == Spoke to the above Motavian after getting the ring
   - End: 0xFF
3. - 0x10 == Asked to fight neifirst (answering is not required, just the dialogue gets here)
   - 0x20 == Governor tells you to get the key cards to the dams (this likely unlocks Piata control tower)
   - 0x40 == Activated the keyboard in Piata Control Tower for the first time
   - 0x80 == Activated the keyboard in Piata Control Tower for the second time (by using the main key)
   - 0x01 == Spoke to Agag in Piata about anti-Mother Brain rebels
   - 0x04 == Examined whirlpool to ademas
   - End: 0xF5
4. - 0x10 == Spoke to Enos in Zema (far west side) once after Zacharia
   - 0x40 == Spoke to Enos in Zema (far west side) a second time after Zachariah
   - 0x80 == Governor gives you the spaceship
   - 0x02 == Eusis talks about gaira satellite after entry
   - 0x04 == Eusis feels change in gaira satellite trajectory
   - 0x08 == Finished cutscene after activating control panel in gaira
   - End: 0xDE
5. - 0x10 == Spoke to Janis in Piata about Professor Ken Miller after speaking to Lot
   - 0x20 == Got the Golden Stone from the Dezorian in Aukbar
   - 0x40 == Gave Golden Stone to Lot
   - 0x80 == Got Enhancer from Lot
   - 0x01 == Spoke to HapsbyXR45 for the first time (just before answering yes or no)
   - 0x02 == Spoke to Lot about enhancer in Piata second time
   - 0x04 == Spoke to Lot about enhancer in Piata first time
   - 0x08 == Spoke to Janis in Piata about Professor Luveno
   - End: 0xFF
6. - 0x01 == Told the dezorian in Ryuon that you believe in the eclipse torch
   - 0x02 == Got the trifluid from the dezorian in Ryuon


(I kept forgetting to watch this as the game progressed, so I'm missing a lot)
Plot counter D byte sequence: 0x247654
1. - 0x10 == ??
   - 0x20 == first convo with governor during opening cutscene
   - 0x80 == ??
2. - 0x40 == Dark Force chest removed
   - 0x80 == Mother brain removed
   - 0x01 == Teim following party of 3 or less
   - 0x02 == Something to do with being in gaira
   - 0x04 == Control tower door opened with musik
3. - 0x10 == Player control on autopilot?
   - 0x01 == ??
   - 0x02 == ??
   - 0x04 == ??
   - 0x08 == ??


Plot counter E byte sequence: 0x247679
1. - 0x02 == Toggle on after Darum cutscene, clone lady tells you about them being admitted, then toggles off


Plot counter F byte sequence: 0x247609
1. - 0x10 == Speak to any acolyte at Esper Mansion except for the left one at the entrance or the two downstairs
   - 0x20 == Picked up all eight Nei armaments
   - 0x40 == Got Nei Sword from Lutz
   - 0x80 == Got Animal Claw from Musk cat
2. - 0x01 == Got Aeroprism from Lutz
   - 0x02 == Gave Dezorian in Aukbar first trifluid
   - 0x04 == Gave Dezorian in Aukbar second trifluid, he gives you heal ring
   - 0x08 == Speak to either acolyte at Esper Mansion entrance (Required to start heal ring side quest) Speaking to any other dezorian toggles this off, but the acolyte can reactivate it at any time.


Dam door tracker: 0x247610
1. - 0x20 == red
   - 0x40 == yellow
   - 0x80 == blue
2. - 0x01 == green


Playthrough tracker: 0x247610
1. - 0x20 == Do not trigger post-Neifirst clone lab dialogue
   - 0x40 == PSGEN1 cleared
   - 0x80 == PSGEN2 cleared

All three bits above must be set to prevent Nei's presence bit from being zeroed. The "Nei Points" counter can be found at 0x248764.


Towns visited bitmap: 0x248754 (Cues new people joining party, allows teleporting)
1. - 0x01: Paseo (required or else teleport just kicks you out)
   - 0x02: Arimaya
   - 0x04: Optano
   - 0x08: Zema
   - 0x10: Kueris
   - 0x20: Piata
   - 0x40: Skure tunnels
   - 0x80: Aukbar
2. - 0x01: Zosa
   - 0x02: Ryuon


Current stage byte: 0x2475C0
Holds various values with different meanings. e.g.
- 0 == Paseo
- 1 == Arimaya
- 2 == Optano
- 3 == Zema
- 4 == Kueris
- 5 == Piata
- 6 == Motavia world map
- 3B == Gaira satellite
- Etc...

This can be used to teleport to various locations by using the save function. As your game is being saved, enter the desired stage, save the game, then reload. You can then manipulate your exact location by poking at the bytes at 0x247689 (x coordinate) and 0x24768D (y coordinate.)

Countown til next battle: 0x248754
This is basically how many "moves" you have until the next battle starts. If you set it to zero, you will start a battle anywhere, even in towns.

Current Meseta: 0x248314
Just how much meseta you currently have.

Current battle enemy HP values:
1. 0x25EB3C
2. 0x25E930
3. 0x25ED48
4. 0x25E724
5. 0x25EF54

As you might notice, the game doesn't display hitpoints above 1,000. These are the actual max hitpoint values for those enemies:
- Neifirst HP: 1,500
- Army Eye HP: 9,999
- DarkForce HP: 11,000
- Mother Brain HP: 7,450
