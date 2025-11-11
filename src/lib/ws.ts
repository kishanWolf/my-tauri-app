export type OutgoingMessage =
  | { type: 'register'; role: 'host'; sessionId: string }
  | { type: 'frame'; sessionId: string; mime: string; chunk: string; w: number; h: number; ts: number }
  | { type: 'cursor'; sessionId: string; x: number; y: number; visible?: boolean }
  | { type: 'control_response'; sessionId: string; viewerId: string; approved: boolean }
  | { type: 'end_session'; sessionId: string }
  | { type: 'webrtc_offer'; sessionId: string; toViewerId: string; sdp: string }
  | { type: 'ice_candidate'; sessionId: string; candidate: any; toViewerId: string };

export type IncomingMessage =
  | { type: 'viewer_joined'; viewerId: string }
  | { type: 'request_control'; viewerId: string }
  | { type: 'release_control'; viewerId: string }
  | { type: 'input_event'; viewerId: string; event: any }
  | { type: 'webrtc_answer'; fromViewerId: string; sdp: string }
  | { type: 'ice_candidate'; fromViewerId?: string; candidate: any }
  | { type: 'error'; message: string }
  | { type: 'privacy_mode_on'; viewerId: string }
  | { type: 'privacy_mode_off'; viewerId: string };

export async function synthesizeInputFromMessage(msg: { type: 'input_event'; viewerId: string; event: any }) {
  // event: {kind:'mouse_move'|'mouse_click'|'key', x?, y?, button?, key?}
  const { event } = msg
  const { invoke } = await import('@tauri-apps/api/core')
  if (event.kind === 'mouse_move') {
    const { xRatio, yRatio } = event
    const w = Math.max(1, captureSize.w)
    const h = Math.max(1, captureSize.h)
    const px = Math.round(Math.max(0, Math.min(1, xRatio ?? 0)) * (w - 1))
    const py = Math.round(Math.max(0, Math.min(1, yRatio ?? 0)) * (h - 1))
    console.log('[HOST] recv mouse ratios ->', { xRatio, yRatio, mappedPx: { x: px, y: py }, desktop: { w, h } })
    await invoke('mouse_move', { x: px, y: py })
  }else if (event.kind === 'mouse_click') {
    console.log('[HOST] recv mouse_click ->', event)
    await invoke('mouse_click', { button: event.button || 'left' })
  } else if (event.kind === 'key') {
    const action = event.action === 'up' ? 'up' : 'down'
    const key = String(event.key || '')
    const code = String(event.code || '')
    const mods = { alt: !!event.altKey, ctrl: !!event.ctrlKey, shift: !!event.shiftKey, meta: !!event.metaKey }
    console.log('[HOST] recv key ->', { action, key, code, mods })
    await invoke('key_event', { action, key, code, mods })
  }
}

let captureSize = { w: 0, h: 0 }
export function setCaptureSize(w: number, h: number) {
  captureSize = { w, h }
}

export function createWebSocket(sessionId: string): WebSocket {
  const serverUrl = import.meta.env.VITE_SERVER_URL || 'http://localhost:8080';
  const wsPath = import.meta.env.VITE_WS_PATH || '/ws';
  const wsUrl = serverUrl.replace('http', 'ws') + wsPath;
  const ws = new WebSocket(wsUrl);
  ws.addEventListener('open', () => {
    const msg: OutgoingMessage = { type: 'register', role: 'host', sessionId } as const;
    ws.send(JSON.stringify(msg));
  });
  return ws;
}


