# Cowork Prompt — Record MARS Demo

> Paste this into Claude Cowork. It will install OBS, record a terminal demo
> of an AI agent discovering and using services on the live MARS mesh network.

---

I need you to record a 45-second terminal demo video of the MARS protocol. Here's exactly what to do:

## Step 1: Install OBS and obs-cmd (if not already installed)

Open PowerShell as admin and run:
```
winget install OBSProject.OBSStudio
pip install obs-cmd
```

If OBS is already installed, skip this.

## Step 2: Configure OBS

1. Open OBS Studio
2. Go to Settings → Output → Recording:
   - Recording Path: `C:\Users\logan\Videos`
   - Recording Format: mp4
   - Encoder: x264
3. Go to Settings → Video:
   - Base Resolution: 1280x720
   - Output Resolution: 1280x720
4. Go to Tools → obs-websocket Settings:
   - Enable WebSocket server: ON
   - Server Port: 4455
   - Enable Authentication: OFF
5. Add a source: Window Capture → select "Windows Terminal" (or whatever terminal you'll use)
6. Close Settings

## Step 3: Open a terminal (Windows Terminal) and set it up

- Set font size to 16-18pt so it's readable in the recording
- Set window size to roughly 100 columns x 30 rows
- Use a dark theme

## Step 4: Start the mesh gateway

In the terminal, run:
```
wsl -e bash -c "cd /home/logan/Dev/mesh-protocol && ./target/release/mesh-gateway --seed 5.161.53.251:4433 --listen 127.0.0.1:3000"
```

Wait 4 seconds for it to bootstrap.

## Step 5: Start OBS recording

Run in a separate PowerShell:
```
obs-cmd recording start
```

## Step 6: Run the demo

Switch to the terminal and run:
```
wsl -e python3 /home/logan/Dev/mesh-protocol/tools/demo/demo.py
```

This will show a live demo with typing effect — discovering search providers, LLM endpoints, MCP tools, and publishing a capability. It takes about 30-40 seconds.

## Step 7: Stop recording

After the demo script finishes (it will show "github.com/mellowmarshall/mars-protocol" at the end and pause), wait 2 seconds, then:
```
obs-cmd recording stop
```

## Step 8: Find the video

The MP4 will be in `C:\Users\logan\Videos\`. Tell me the filename.

---

## IMPORTANT NOTES:
- The demo queries a LIVE mesh network — the results are real, not mocked
- If the gateway shows "bootstrap complete, discovered=3" that means it's connected
- If discover results are empty, the cron job may not have run yet — wait a few minutes and try again
- The video should be 720p, roughly 45 seconds, showing the full terminal output
