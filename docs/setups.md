# Setups

Step-by-step recipes for common uses. They assume the default channels
(Microphone, System, Browser, Voice Chat, Music, Games, Sound Effects) and the
two default mixes, **My Mix** and **Stream**. Rename or add whatever you like.

**The idea in one sentence:** you hear the mixes with 🎧 on; everyone else
(your stream, your call, your recording) gets whichever mix you pick as their
microphone.

## Streaming with OBS

Goal: viewers hear your mic, game and music; you hear game, music and your
Discord call, but not yourself.

1. Put your apps in channels: click **Games** and tick your game, click
   **Music** and tick Spotify, click **Voice Chat** and tick Discord.
2. In the **Stream** column, set the levels your viewers get: Microphone on,
   Games and Music on. Take **Voice Chat** out of Stream (right-click the cell
   → *Remove from this mix*) if your call shouldn't be on stream.
3. In **My Mix**, keep the Microphone muted (red) so you don't hear yourself,
   and set Games, Music and Voice Chat to taste. Make sure 🎧 is on for My Mix.
4. In OBS: *Sources → + → Audio Input Capture → LinuxLink Stream*. Remove or
   mute OBS's *Desktop Audio* and *Mic/Aux* sources so nothing is doubled.

Music too loud on stream but fine for you? Lower Music only in the Stream
column. That's the point of separate mixes.

## Discord or other voice calls

Goal: the people in your call hear your microphone (and optionally music or a
game), without hearing themselves back.

- **Just your voice:** in Discord, *User Settings → Voice & Video → Input
  Device*: keep your real microphone, or pick *LinuxLink Stream* to send them
  your whole Stream mix instead.
- **Your voice plus music for friends:** add a mix (click **+** next to the
  last mix header), name it *Call*, route Microphone and Music into it, and
  pick **LinuxLink Call** as Discord's input device. Never route **Voice Chat**
  into the mix Discord listens to, or your friends hear themselves.
- Put Discord itself in **Voice Chat** so you can set its volume per mix.

Tip: Discord's *Automatic Gain Control* changes your system microphone volume
by itself. If your mic level keeps jumping, turn it off in *Voice & Video*.

## Recording a podcast or video

1. Add a mix called *Recording* (the **+** next to the mix headers).
2. Route only what belongs in the recording: Microphone, and maybe System or a
   guest's Voice Chat.
3. In your recorder (OBS, Audacity, Kdenlive…), record from **LinuxLink
   Recording**.
4. Leave 🎧 off for the Recording mix unless you want to hear it.

## Game and chat at their own volumes

You don't need to stream to benefit: put your game in **Games** and Discord in
**Voice Chat**, then balance them in **My Mix**. The channel's master fader
(in the first column) changes the channel everywhere; the cell in My Mix
changes it only for you.

## Headphones and speakers

- Pick your listening device in the top-right corner, or simply switch output
  in your desktop's volume applet: LinuxLink follows.
- **Bluetooth headsets switch automatically.** Connect one and your mixes move
  to it; disconnect it and LinuxLink carries on with the previously connected
  headset, then your previous output.
- Want something on speakers *and* headphones? Open a mix's settings (click
  its header) and add the other device under **Extra outputs**.

## Hearing yourself (mic monitoring)

Unmute the Microphone cell in **My Mix**. Keep it low: Bluetooth headsets add
noticeable delay, wired headphones much less. Lowering *Settings → Audio →
Latency* reduces the delay.

## Several microphones

Use *Create channel → Input device* for each, or click a device under **Your
devices** in the sidebar and choose **Add as channel**. Each input channel's
master fader is that device's real volume.
