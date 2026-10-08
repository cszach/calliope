// Runs at document start in the isolated "muse-client" world.
// Records whether this WebKit build exposes WebRTC so the app can show a
// banner when Muse features that need it (voice calls, live view) are used.
(() => {
  try {
    window.__museHasRTC = typeof RTCPeerConnection !== "undefined";
  } catch (_e) {
    window.__museHasRTC = false;
  }
})();
