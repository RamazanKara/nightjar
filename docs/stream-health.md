# Stream health

[Nightjar docs](index.md) › [Troubleshooting](index.md#troubleshooting)

**Why did my stream stutter?**, the Stream Doctor, names what made the stream stutter in plain words, with the number behind it, offers fixes for this stream that one tap undoes, and says when an upscaler costs more than it gives. Open it on the Control Center's Picture page: on Android under **Diagnostics**, on Windows, Linux and macOS directly on the page.

## What it names

The card leads with one finding, the stage that cost the stream its smoothness, and the measurement behind it:

| Finding | The number it shows |
| --- | --- |
| Wi-Fi or network: frames arrive unevenly | How many frames were lost, or in how many of the last seconds frames arrived bunched |
| PC: sending the video takes too long | How many pictures left the PC late, and the time lost |
| PC: the GPU or encoder falls behind | How long the PC took per frame, beside how long a frame lasts |
| This device: the decoder falls behind | How long decoding took, beside how long a frame lasts |
| Screen: frames are shown late | How long frames waited to be shown, beside how long a frame lasts |
| This device: the receive buffer is too small | How much receive buffer the system granted of what Nightjar asked for |

Until it has a few seconds of video it reads **Checking your stream**, and with nothing repeated in the last 30 seconds **Your stream is running smoothly**. Advice follows the finding: a cable or a less busy 5 or 6 GHz Wi-Fi channel, background Wi-Fi scans when stalls come on a beat, closing other work on the PC or capping the game's frame rate, a lower frame rate or another frame pacing mode for the next stream, or letting this device cool down.

With a PC that offers its own measurements (Rubylight), the finding also uses the PC's stage report: how many pictures left the PC late and the time lost in capture, colour conversion, encoding, packetizing and sending, under **Show technical measurements**. Other PCs get the finding from this device's measurements alone.

## Fixes for this stream

Where it helps, the card offers one-tap fixes that last for this stream only and change no setting:

- **Lower bitrate to** a fifth less **(this stream)**, through the PC's own doctor where it offers one, otherwise as the classic bitrate trial below.
- **Lower resolution to** one step down **(this stream)**, with a PC that can change a running stream's size. If the PC can't change it or keeps the size, the card says so and nothing changes.

Each fix turns into its undo once applied (on Android **Undo: back to** the earlier bitrate or size; on the desktop **Undo bitrate trial** and **Undo resolution change**). Only the device that started the stream can apply them; a guest sees the finding without the fixes.

## Upscalers

When a lower resolution would help, the card names the upscaler that keeps the picture sharp at it for the next stream (FSR or SGSR, MetalFX on a Mac). When the upscaler in use costs more than it gives, it says so: the stream is already as large as the screen, or the screen or the decoder falls behind with it on, with a lighter upscaler to try or the advice to turn it off for the next stream.

## The measurements

The report uses up to 30 seconds of stream measurements, the start times of network stalls from the last 20 minutes, and the current Wi-Fi reading. It does not move the mouse, record video, contact any service, or write a history to disk. Copying the report puts only the displayed evidence and advice on the clipboard.

The report identifies network loss and variation, slow decoding, host processing, display waits and Android thermal pressure from repeated observations; several findings can appear together. When the host sends timing reports, the host time runs from the frame to its last packet sent, so it includes waiting and sending, not only encoding. Display waits are judged from decode to on screen, the compositor included, and compared with the frames actually released rather than the requested frame rate, so a still or variable-refresh desktop does not count as dropped frames. Missing counters stay unavailable, and an empty report does not prove that every stage is healthy. The stage timings cannot be added up to an input-to-screen latency.

Play for a few seconds, then open the report right after a stutter. Sampling continues while menus are open and pauses while the app is in the background or a host action runs. Stage counters older than two seconds are not used, old history expires, and advice is withheld when there is no recent video.

The classic bitrate trial needs a host that supports bitrate changes during a stream (Rubylight 3.0): it lowers the encoder bitrate by 20%, to no less than 500 kbps, for this session. Play similar content for another 30 seconds, then return and undo it if nothing improved. It is not offered while **Lower bitrate when the network is busy**, automatic quality or a manual change controls the bitrate. With other hosts the card gives the same findings and advice without the bitrate fix.

## Wi-Fi facts

The report ends with a **Connection** section, and the **Debug** performance overlay lists the same lines under Network:

- **Wi-Fi band**, channel and channel width. The width is what the access point announces about its channel (Android 12 and later); Android offers no width field for the connection itself, so older versions show *not available*.
- **Wi-Fi signal** in dBm with a grade: excellent from −55 dBm, good from −67 dBm, fair from −75 dBm, weak below that. A weak signal adds a hint to move closer to the router.
- **Wi-Fi link speed** for sending and receiving (Android 10 and later; older versions show one combined rate). Link speed is the radio's current rate, not the stream's throughput.
- **Wi-Fi standard** (Wi-Fi 4 to 7, Android 11 and later).
- **Low-latency Wi-Fi lock**: whether Nightjar holds Android's low-latency Wi-Fi lock for this stream. The lock exists on Android 10 and later.
- A **2.4 GHz warning**: that band is slower and more crowded than 5 or 6 GHz and a common cause of stutter.

Ethernet, mobile data and VPN connections say so instead. Behind a VPN such as Tailscale, Nightjar shows the Wi-Fi facts when the device has a Wi-Fi association. Anything Android does not report reads *not available* rather than disappearing.

Nightjar reads these values every five seconds on a low-priority thread while a stream is connected; the overlay and the report use the latest reading, so the video, audio and input paths are unaffected. Only the network and Wi-Fi state permissions are needed. The network name (SSID) and the access point's address are not shown: Android treats them as location data, and Nightjar does not request location permission for this.

## Stalls every few minutes

A "lag spike every two minutes" is often a background Wi-Fi scan: the radio leaves the channel for a moment to look for other networks. Nightjar keeps the start time of each network stall (frame loss of at least 2% or round-trip variation above a frame interval) for 20 minutes. When at least four stalls fall on a steady beat between 30 seconds and 10 minutes apart, allowing one missed beat and a few unrelated stalls, the report adds **Repeating stalls** with the period, and the Debug overlay shows **Stall rhythm**. A rhythm needs a few minutes of streaming to show, so it appears even when the last 30 seconds were clean.

To reduce scans:

- On Android, open **Settings → Location → Location services** and turn off **Wi-Fi scanning** and **Bluetooth scanning**. The names and path vary by device.
- Leave **Developer options → Wi-Fi scan throttling** on, so apps cannot scan often.
- Close apps that look for networks or nearby devices while you stream.
- On the PC, use a cable, or stop tools that scan for Wi-Fi networks.

Nightjar already holds Android's low-latency Wi-Fi lock while streaming, which asks the system to defer scans and power saving. Some devices scan anyway, which is why the rhythm check exists.

## Frames in bursts

Some Wi-Fi links hold video back for a few tens of milliseconds and then deliver several frames at once. The picture then hitches once per burst. Nightjar compares each frame's arrival with the host's timestamps: a frame that comes at least three quarters of a frame interval (8 ms at the least) later than its timestamp says, after the frame before it, is late. Frames whose timestamps step as far as their arrivals, such as a still desktop, a game that slows down or a capture stall, do not count. The timestamps are taken at capture, so a frame the host itself sent later than the frame before (a slower encode, a wait for its turn, slower sending after a loss) also arrives late. When the host sends timing reports, Nightjar matches each frame with its report and takes off how much later after its capture the host sent its first packet than the frame before's; a frame counts only if it is still late after that. A frame whose report is missing, or a host without timing reports, takes off the change in the frame header's host processing time instead. A sample with late frames is a vote; a run of up to 10 such samples in a row counts as 2 votes, because a launch or a switch to fullscreen holds frames back for a few seconds once and is not reported. When the votes reach 6 and a third of the samples, the report adds **Bursts**.

[Documentation](index.md) · [Video and latency](video-and-latency.md) · [Troubleshooting](TROUBLESHOOTING.md)
