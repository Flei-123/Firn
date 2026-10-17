# JARVIS Android app - release notes

One section per version, newest first: `## <version>`, then short bullet lines (English).
`tools/android/jarvis_release.sh` puts the section of the released version into the signed
store entry (field `aenderungen`, at most 1536 bytes); the app shows it in Settings.

## 0.2.2
- Settings shows the update channel, the installed version, the available version and these release notes.
- Channel switch (stable / test) appears only when the store offers both for this app.

## 0.2.1
- Push over HTTPS (long poll), tap on a notification opens the chat it belongs to.
- Chat list, photo upload (up to 3 MB), file receive into Downloads/Jarvis.

## 0.2.0
- Board tab: projects with roadmap progress, needs (answer in the app), workers.
- Tasks tab and Devices tab; voice output; sign in with server, user and password.
