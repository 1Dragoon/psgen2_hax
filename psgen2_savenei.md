## Preamble -- How I figured this out.

While going through another Nei revival guide, I had a few FOMO moments that I might miss out on some dialogue elements and make it all the way to Neifirst only to have to start over. That lead me into wondering if there's a way I could figure out how the game keeps track of plot progress so that, if I did miss anything, I could figure out what I missed later.

As soon as I got a general idea for how the game tracks plot progress, I suspected very quickly that most of what the guide has you do probably isn't needed. Nevertheless, I followed it anyways, taking notes of each plot bit as they were set, figuring maybe they had to be in a certain state by the time you reach Neifirst.

After saving Nei, I started poking around at the memory to see if I could "un-save" her, and quickly noticed that zeroing all of the plot bits didn't accomplish that. So then, what would? Was my hunch wrong? Was it tracking these steps in some more complicated way? Well no, in fact it was even more simple than my original hunch. I kept zeroing out various memory blocks until I finally managed to "un-save" Nei, and eventually narrowed it down to a single byte. That byte had the value 0x13.

A value of 19 (decimal) does not and can not hold enough information to ensure all of these dialogue events were met (unless maybe it was a "countdown".) I started loading previous save points and noticing that the value incremented over time, and eventually I fugured out that it's a simple incrementing counter. Much like a score tracker to keep track of points. After experimenting, I determined that you need greater than or equal to 19 points to save Nei, in addition to the already known about starting with a PSGEN1 and PSGEN2 playthrough, which is tracked in another byte I had found much earlier.

I tested all of this by playing from the beginning again, only with the random encounters disabled so it doesn't take forever and a day. It didn't take long to find out how to get all 19 of these "Nei points".

### Technical Findings:

This is just an explainer for how the game determines whether you get to keep Nei. Skip this section if you don't care about that.

After Neifirst, there's an anime cutscene. After that cutscene ends, the game immediately checks two values:
- How many...let's call them "Nei points", you have. This value is at 0x00248764.
- The first two bits of...let's call it a "playthrough tracker" byte. This value is at 0x00247660.

"Nei points", unlike the plot progression trackers that are packed bitfields, is just a normal integer that increments each time you engage in certain conversations. These are indicated by putting a bold number of the total points you should have by then in the guide.

When you start a new game without loading a PSGEN1 clear, the "playthrough tracker" is zero. Loading a PSGEN1 clear sets the leftmost bit to one. If you start with a PSGEN2 clear save, the second to left bit starts at one.

These are the required conditions:
- Nei points have to be greater than or equal to 19, or 0x13.
- Playthrough tracker bits have to be 1100-0000 (little endian), or 0xC0. Other bits can be turned on with no ill effects that I've observed.

The game logic doesn't seem to care when or how these values are set, just that they hold the right values at the end of the aforementioned cutscene.

When the game checks these values and the conditions are met, it flips the third bit in the playthrough tracker, making it 0xE0. Later on, when the game teleports you to Paseo from AMeDAS, during the scene transition it checks that all of those first three bits are on. If they're not, the game flips Nei's "character playable" bit to zero, removing her from the game.

All characters, including Eusis, have such a bit. The six characters besides Eusis and Nei all start with them at zero, and they're turned on in their character sheet struct at the start of their respective cutscenes.

You can literally start a whole new game without doing any previous playthrough, set these values, teleport yourself to neifirst (yep, there's a way, out of scope for this document), beat her, and Nei lives. I've tried exactly this, and it works.

NB: Something even more simple you can do if you just want a guaranteed ability to keep Nei: Simply set the playthrough tracker to 0xE0 at any point in the game before you're automatically teleported out of AMeDAS...and Bob's your uncle. This should work by altering it in the savegame on a real PS2 as well, though I don't own any real hardware to try this with.

# The Guide

Below is a nearly complete guide for keeping Nei after the battle with Neifirst. I've only really ommitted the details for navigating on the world map and in most of the dungeons (except Uzo and everything above 4F in AMeDAS because these rate too high on the "complicated maze just to make you spend a long time on it" scale for my taste, though I do like the overall motif of Uzo Island.) I indicate where each point is obtained by putting the number in bold and parenthesized. The number is the total number of points you should have after completing the action on that line, assuming you did them in order.

Note that for the most part, order isn't that important. For example I've been able to add some characters to my party, remove them, and THEN speak to the governor about them. The only thing that should bother it is if you get another character before hearing the previous character's back story which WILL be a deal breaker, however caveat emptor as I haven't thoroughly tested it. But in general, the game won't allow you to advance the plot too far to do many of these things, with only two notable exceptions that I've mentioned below.

However, following this guide in the order I've laid it out has always worked in my tests.

#### Paseo, after intro cutscene:

1. Consult twice. (**1**)
2. Consult a third time right after (**2**)
> Make sure you do not leave or enter any towns between consultations. Doing this resets the consultation progress and any other ephemeral tracker bits.
3. Go to Arimaya.

#### Arimaya:

1. Only one of these actions is required:
  - Esther is a woman with light-brown hair in a white shirt and purple skirt on the far east. Speak to her at least twice.
  - Michael is a kid with green hair in yellow clothing in the southeast corner. Speak to him at least once.
2. Consult after doing one or both of the above but BEFORE you take the next action. (**3**)
3. Peter is a light-green haired kid in a brown shirt on the far north side. Speak to him, then consult. (**4**)

#### Paseo

1. Go home to start the Rudger cutscene.
2. Talk to the governor without Rudger in your party to hear his back story. (**5**)
3. Just north of Eusis's home is a redhead named Cain. Talk to him about the orphanage. (**6**)
> Add Rudger to the party if he isn't already.
4. Titus is a brown haired dude in a blue shirt in the far northwest corner. Talk to him and he'll talk with Rudger about firearms. (**7**)
> Rudger won't be needed again until you have at least 11 points, but keeping him in your party doesn't hurt.

#### Shuren Factory

1. Get the ransom letter from the rascals' leader on 4F and the two sticks of dynamite on 1F.

#### Nido Tower

> This dungeon has one of two trimates available early in the game. It's a bit out of the way to obtain though, the one in the biosystems lab is quicker to get. Either way, you'll need at least one later on before you're able to buy one.
1. After giving Teim the ransom letter, go to the North Bridge.
>> Side note: Other guides have you go to the North Bridge before Teim. That method has some small Nei back story in it, but it's mutually exclusive with some other dialogue involving Teim that can only be had by skipping that step and consulting AFTER you get Teim. Neither method grants you points, so either method or not doing either at all is totally fine. There are actually a few mutually exclusive plot points like this in the game, which further confirmed to me at the time that the idea of seeing as much dialogue as possible coulnd't be how Nei is saved.

#### North Bridge

1. After the cutscene with Teim and Darum, consult twice. (**8**)
> No more consulting is necessary from this point onward.
2. Go to Optano, enter it, then return to Paseo.

#### Paseo

1. Go home to start the Anne cutscene.
2. Talk to the governor without Anne in your party to hear her back story. (**9**)
> Anne isn't needed yet, but you may add her to the party.

#### Biosystems Lab

1. When you reach B1F after falling through the vent, on your way to the System Recorder you'll see a chest in the northwest side. There's a trimate in it. Pick it up but DO NOT USE IT! You'll need it later.
2. Get the System Recorder and return to Paseo.

#### Paseo

1. Give the System Recorder to the governor, go the library and hear about the results, go back to the governor again to hear about Huey.
2. Go home to start the Huey cutscene.
3. Talk to the governor without Huey in your party to hear his back story. (**10**)
> You may need to speak with the governor a few times before hearing about Huey.
> Huey isn't needed yet, but you may add him to the party.
4. Go to Zema with Anne in the party and at least one trimate.

#### Zema

> Note: Do not advance the plot too far or else you won't be able to do this!
1. Look for Uriel, a kid with light green hair in brown just west of the teleport station. Anne will talk to him and you'll give him a trimate. (**11**)
> Anne is no longer needed for points.
2. Go back to Paseo

#### Paseo

1. Go home to start the Amia cutscene.
2. Talk to the governor without Amia in your party to hear her back story. (**12**)
> Amia isn't needed yet, but you may add her to the party.
3. Go to Zema and enter from the east side.

#### Zema

> Everything we do in this section is only for the purpose of getting to Kueris. I.e. simply doing the mandatory steps to advance the plot.
1. You need to do both of these things, but you may do them in any order
  - Look a for black haired kid in orange named Zachariah on the far east side. Talk to him twice. Slightly north of Zachariah is a woman in pink named Deborah. Talk to her.
  - Slightly north of Deborah is a woman with black hair and a green shirt named Carmel. Talk to her twice.
2. Just over the bridge west of them near the weapon shop is a man with light-brown hair wearing a blue shirt named Luke. Talk to him. He'll tell you to talk to Enos.
3. Enos is an old man in green on the west side. Talk him and Agree to his request.
4. Go to Kueris.

#### Kueris

1. Enter Kueris and then go back to Paseo.

#### Paseo

1. Go home to start the Keinz cutscene.
2. Talk to the governor without Keinz in your party to hear his back story. (**13**)
> Keinz isn't needed yet, but you may add him to the party.
3. Go back to Kueris.

#### Kueris

1. Talk to Nanette to get her letter. She's the purple haired woman in orange west of the teleport station, sometimes hidden behind the buildings.
2. Go to Zema and enter from the west side.

#### Zema

1. Talk to Enos to get the `Maruera Berries`.
2. Go to Kueris, enter from the south side.

#### Kueris

1. At the south side of Kueris is a house surrounded by walls. Just south of it is a Motavian walking around. Talk to this Motavian to give him the `Maruera Berries`. Talk to him again to get the `Motavian Ring`.
2. If Rudger and Amia aren't in your party, go back to Paseo and get both of them.
3. Go to Optano.

#### Optano

1. Look for a black haired man with an orange and yellow shirt named Phillip. Talk to him to get the Silver Bullet Necklace. (**14**)
> You can actually do this immediately after dropping the trimate in the Urinal, but like most things in this guide, there's no rush. The only requirement to start this dialogue is having Rudger with you and at least 11 points.
2. Go to the far southeast and talk to the Motavian with Amia in your party about an incident the governer referred to earlier. (**15**)
> Amia and Rudger are no longer needed for points.
3. Go to Roron Dump and enter from the north side of the building.

#### North Roron Dump

1. Go to B3F and talk to the Motavian on the southern end of the floor.
2. Go back to Zema.

#### Zema

1. Find an old man named Solomon, very close to where you found Zachariah. He'll offer you the `Polymetryl` for 10k Meseta, which you'll need to buy.
2. Go back to Roron Dump, enter from the south side of the building.

#### South Roron Dump

1. Go to B3F and talk to either of the Motavians on the south side. Now talk to the other one to give him the `Polymetryl`.
2. Speak to either Motavian twice more to get the `Jet Scooter`.
3. Leave Roron and get on the `Jet Scooter`. Take it to Piata.
4. Enter Piata and return to Paseo.

#### Paseo

1. Go home to start the Silka cutscene.
2. Talk to the governor without Silka in your party to hear her back story. (**16**)
3. Put Silka and Keinz in your party and go to Kueris.

#### Kueris

1. With Silka in your party, find the Motavian near the hospital and talk to him. (**17**)
> Silka is no longer needed for points.
2. Go to Piata and enter from the north side.

#### Piata

1. With Keinz in your party, talk to Janis, a woman with purple hair on the west side but east of the three towers. (**18**)
> Keinz is no longer needed for points.
2. Optional for Enhancer: Speak to Lot twice, an old man in purple walking around near Janis. Then speak to Janis again and hear about a roboticist named Ken Miller.
3. The next two steps can be done in any order.
 - Speak to Agag, a black haired man in the far south east corner about anti-mother brain rebels. Sometimes he's hidden behind a tall building.
 - Speak to Abel, an old man in blue near the armor shop about the space accident.
4. Speak to Gabrielle, a blue haired woman in a light green shirt and purple skirt near the clone labs.
5. Go to Uzo Island.

#### Uzo Island

- Fastest way to the Maruera Tree:
  1. Go up the first two stairs you see and into the first cave you see.
  2. Go east, skipping the first stairs you see and into the first cave you see.
  3. Go east and up the stairs and into the cave above.
  4. Go west until you see a cave, enter it.
  5. Go east and up the stairs, and up the next set of stairs after that.
- Get the leaves and head to Kueris.
> After you leave Uzo, add Huey to your party if he isn't in it already.

#### Kueris
1. Visit Hiram and give him the `Maruera Leaves` in exchange for the `Maruera Gum`.
2. After receiving the `Maruera Gum`, re-enter Hiram's house with Huey in your party. (**19**)

> You're now guaranteed to be able to keep Nei after defeating Neifirst no matter what you do (or don't do) at this point. You may now head directly to AMeDAS to continue the game.

#### AMeDAS

After reaching 5F:
1. Go to the far northwest corner of the map by first going slightly north, then as far west as you can, then as far north as you can. Take the stairs up.
2. Go directly south but don't run into the pit!. Go east until you can first go north, then snake around back west, follow the path until you can go to 7F.
3. Go south and then go east, ignoring the stairs you see to the north. Go south when you can and take the stairs down to 6F.
4. Go south and take the first stairs up that you see.
5. Go west as far as you can and take the first stairs up that you see.
6. Go east and save the Nei!

#### Keeping Nei

Notice I don't call it resurrection. This is mainly because the game's behavior doesn't really change after this point. You can simply clone her like usual, that's really it. No additional story elements or cutscenes happen that I've (yet) seen. The end result is the game simply doesn't flip her "usable character" bit to remove her, and the clone labs don't talk about cells being degraded.

It's worth noting that the Nei points may have more use in the game. Although the first 19 points are entirely optional and can be missed, there is a 20th point issued after the anime cutscene while AMeDAS is blowing up. It doesn't appear to be used for anything by this point, and having only 18 points before Neifirst does not allow you to keep Nei either.

The game has some items in it with an unknown purpose and no obvious means of obtaining them. Take for example the Land Master, Ice Digger, and Atlas. They could just be remnants from PSGEN1, which the developers likely used as a starting point to develop this game, and perhaps they simply forgot to remove these items? Some items, like the Animal Claw, are issued after the battle with Neifirst, so maybe these items see something similar? Anybody's guess at this point.
