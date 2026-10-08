// Runs at document start in the isolated "calliope" world, on every page.
// WebKitGTK 2.54 has no WebRTC. When a page fails because of that, tell the
// app so the tab can offer to open the page in a browser that has it.
//
// This only listens for errors that name WebRTC classes; it never defines or
// wraps them, so pages that check for WebRTC and fall back (Muse's voice mode
// does) behave exactly as they would without it.
(() => {
  const WEBRTC = /\b(webkit)?RTC(PeerConnection|SessionDescription|IceCandidate|DataChannel|RtpSender|RtpReceiver)\b/;
  let reported = false;
  const check = (text) => {
    if (reported || !WEBRTC.test(String(text))) return;
    reported = true;
    window.webkit.messageHandlers.calliopeWebRTC.postMessage(location.href);
  };
  window.addEventListener('error', (e) => check(e.message), true);
  window.addEventListener('unhandledrejection', (e) => {
    const r = e.reason;
    check(r && typeof r === 'object' ? r.message : r);
  });
})();
