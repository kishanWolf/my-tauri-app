import { useEffect, useMemo, useRef, useState } from 'react'
import './App.css'
import { createWebSocket, type IncomingMessage, synthesizeInputFromMessage, setCaptureSize } from './lib/ws'

function App() {
  const [sessionId, setSessionId] = useState<string>('')
  const [connected, setConnected] = useState(false)
  const [pendingControlViewer, setPendingControlViewer] = useState<string | null>(null)
  const [w, setW] = useState(0)
  const [h, setH] = useState(0)
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const wsRef = useRef<WebSocket | null>(null)
  const pcsRef = useRef<Record<string, RTCPeerConnection>>({})
  const dcsRef = useRef<Record<string, RTCDataChannel>>({})
  const streamRef = useRef<MediaStream | null>(null)
  const capturePromiseRef = useRef<Promise<MediaStream> | null>(null)
  const joiningViewersRef = useRef<Set<string>>(new Set())
  const resizeHandlerRef = useRef<(() => void) | null>(null)

  const serverUrl = useMemo(() => import.meta.env.VITE_SERVER_URL || 'http://localhost:8080', [])
const apiUrl = useMemo(() => import.meta.env.VITE_API_URL || 'http://localhost:8080', [])
  useEffect(() => {
    return () => { wsRef.current?.close() }
  }, [])

  // Host will join an existing session created by the viewer
  async function joinExistingSession() {
    if (!sessionId) return false;
    // validate
    try {
      const res = await fetch(`${apiUrl}/api/viewer/validateSession/${sessionId}`);
      if (!res.ok) {
        console.error('invalid session id');
        return false;
      }
      return true;
    } catch (e) {
      console.error('session lookup failed', e);
      return false;
    }
  }

  async function connectWs() {
    if (!sessionId) return;
    
    // First validate the session
    const isValid = await joinExistingSession();
    if (!isValid) {
      console.error('Cannot connect to invalid session');
      return;
    }
    
    const ws = createWebSocket(sessionId);
    wsRef.current = ws;
    ws.addEventListener('open', () => {
      setConnected(true);
      // Update host status to active when connected with a small delay
      // to ensure registration is processed
      setTimeout(() => {
        fetch(`${apiUrl}/api/viewer/updateHostStatus/${sessionId}/true`, { 
          method: 'POST' 
        }).catch(err => {
          console.error('Failed to update host status to active:', err);
        });
      }, 100);
    });
    ws.addEventListener('close', () => {
      setConnected(false);
      // Update host status to inactive when disconnected
      fetch(`${apiUrl}/api/viewer/updateHostStatus/${sessionId}/false`, { 
        method: 'POST' 
      }).catch(err => {
        console.error('Failed to update host status to inactive:', err);
      });
    });
    ws.addEventListener('message', async (ev) => {
      const msg = JSON.parse(ev.data) as IncomingMessage
      if (msg.type === 'request_control') {
        setPendingControlViewer(msg.viewerId)
      } else if (msg.type === 'release_control') {
        setPendingControlViewer(null)
      } else if (msg.type === 'input_event') {
        // Tauri OS input
        try { await synthesizeInputFromMessage(msg as any) } catch {}
      } else if ((msg as any).type === 'privacy_mode_on') {
        try {
          const { invoke } = await import('@tauri-apps/api/core')
          await invoke('create_privacy_overlay')
        } catch (e) {
          console.error('Failed to enable privacy overlay', e)
        }
      } else if ((msg as any).type === 'privacy_mode_off') {
        try {
          const { invoke } = await import('@tauri-apps/api/core')
          await invoke('destroy_privacy_overlay')
        } catch (e) {
          console.error('Failed to disable privacy overlay', e)
        }
      } else if ((msg as any).type === 'viewer_joined') {
        await onViewerJoined((msg as any).viewerId)
      } else if ((msg as any).type === 'webrtc_offer_request') {
        await onViewerJoined((msg as any).fromViewerId)
      } else if ((msg as any).type === 'webrtc_answer') {
        const fromViewerId = (msg as any).fromViewerId
        const pc = pcsRef.current[fromViewerId]
        if (pc) {
          try { await pc.setRemoteDescription({ type: 'answer', sdp: (msg as any).sdp }) } catch {}
        }
      } else if ((msg as any).type === 'ice_candidate' && (msg as any).fromViewerId) {
        const fromViewerId = (msg as any).fromViewerId
        const pc = pcsRef.current[fromViewerId]
        if (pc && (msg as any).candidate) {
          try { await pc.addIceCandidate((msg as any).candidate) } catch {}
        }
      }
    });
  }

  async function ensureScreenShare(): Promise<MediaStream | null> {
    if (streamRef.current) return streamRef.current
    if (capturePromiseRef.current) return await capturePromiseRef.current
    
    // Get display media normally
    try {
      const stream = await (navigator.mediaDevices as any).getDisplayMedia({ 
        video: { 
          frameRate: 30
        } 
      })
      
      streamRef.current = stream
      // Initialize mapping from viewer ratios to desktop pixels (use device pixels)
      const updateCaptureSize = () => {
        const dpr = Math.max(1, window.devicePixelRatio || 1)
        const dw = Math.round((window.screen?.width || 0) * dpr)
        const dh = Math.round((window.screen?.height || 0) * dpr)
        setCaptureSize(dw, dh)
      }
      updateCaptureSize()
      // Keep in sync if resolution/DPI changes
      const onResize = () => updateCaptureSize()
      window.addEventListener('resize', onResize)
      resizeHandlerRef.current = () => {
        window.removeEventListener('resize', onResize)
      }
      
      return streamRef.current
    } catch (error) {
      capturePromiseRef.current = null
      throw error
    }
  }

  async function startScreenShare() {
    if (!streamRef.current) {
      await ensureScreenShare()
    }
    const canvas = canvasRef.current as HTMLCanvasElement | null
    if (canvas) {
      const video = document.createElement('video')
      video.srcObject = streamRef.current as MediaStream
      await video.play()
      const ctx = canvas.getContext('2d') as CanvasRenderingContext2D | null
      function tick() {
        if (video.videoWidth && video.videoHeight && canvas && ctx) {
          canvas.width = video.videoWidth
          canvas.height = video.videoHeight
          setW(video.videoWidth)
          setH(video.videoHeight)
          ctx.drawImage(video, 0, 0)
        }
        requestAnimationFrame(tick)
      }
      tick()
    }
  }

  async function onViewerJoined(viewerId: string) {
    if (!wsRef.current || !sessionId) return
    // Prevent duplicate handling if multiple events arrive for the same viewer
    if (pcsRef.current[viewerId]) return
    if (joiningViewersRef.current.has(viewerId)) return
    joiningViewersRef.current.add(viewerId)
    try {
      if (!streamRef.current) {
        await ensureScreenShare()
      } 
    const iceServers = (() => {
      const turnUrl = (import.meta as any).env.VITE_TURN_URL
      const turnUser = (import.meta as any).env.VITE_TURN_USER
      const turnCred = (import.meta as any).env.VITE_TURN_CRED
      const list: RTCIceServer[] = [{ urls: ['stun:stun.l.google.com:19302'] }]
      if (turnUrl && turnUser && turnCred) list.push({ urls: [turnUrl], username: turnUser, credential: turnCred })
      return list
    })()
    const pc = new RTCPeerConnection({ iceServers })
    pcsRef.current[viewerId] = pc
    // Create a data channel for control events from viewer
    const dc = pc.createDataChannel('control')
    dcsRef.current[viewerId] = dc
    dc.onmessage = async (ev) => {
      try {
        const msg = JSON.parse(ev.data)
        if (msg && msg.type === 'input_event') {
          await synthesizeInputFromMessage(msg)
        }
      } catch {}
    }
    if (streamRef.current) {
      streamRef.current.getTracks().forEach(t => pc.addTrack(t, streamRef.current as MediaStream))
    }
    pc.onicecandidate = (e) => {
      if (e.candidate) {
        wsRef.current!.send(JSON.stringify({ type: 'ice_candidate', sessionId, candidate: e.candidate.toJSON(), toViewerId: viewerId }))
      }
    }
    pc.oniceconnectionstatechange = async () => {
      const state = pc.iceConnectionState
      console.log('[HOST] iceConnectionState=', state)
      if (state === 'failed' || state === 'disconnected') {
        try {
          const offer = await pc.createOffer({ iceRestart: true })
          await pc.setLocalDescription(offer)
          wsRef.current!.send(JSON.stringify({ type: 'webrtc_offer', sessionId, toViewerId: viewerId, sdp: offer.sdp }))
        } catch {}
      }
    }
    pc.onconnectionstatechange = () => {
      const cs = (pc as any).connectionState
      console.log('[HOST] connectionState=', cs)
    }
    const offer = await pc.createOffer()
    await pc.setLocalDescription(offer)
    wsRef.current.send(JSON.stringify({ type: 'webrtc_offer', sessionId, toViewerId: viewerId, sdp: offer.sdp }))
    
    // Automatically approve control for this viewer
    setTimeout(() => {
      if (wsRef.current && sessionId && viewerId) {
        wsRef.current.send(JSON.stringify({ type: 'control_response', sessionId, viewerId, approved: true }))
      }
    }, 100)
    } finally {
      joiningViewersRef.current.delete(viewerId)
    }
  }

  function broadcastCursor(ev: React.MouseEvent) {
    if (!wsRef.current || !sessionId) return
    const rect = (ev.target as HTMLCanvasElement).getBoundingClientRect()
    const x = ev.clientX - rect.left
    const y = ev.clientY - rect.top
    wsRef.current.send(JSON.stringify({ type: 'cursor', sessionId, x, y, visible: true }))
  }

  function endSession() {
    // Notify server if possible
    if (wsRef.current && sessionId) {
      try { wsRef.current.send(JSON.stringify({ type: 'end_session', sessionId })) } catch {}
    }
    // Close data channels
    Object.values(dcsRef.current).forEach((dc) => {
      try { dc.close() } catch {}
    })
    dcsRef.current = {}
    // Close peer connections
    Object.values(pcsRef.current).forEach((pc) => {
      try { pc.getSenders()?.forEach((s) => { try { s.track && s.track.stop() } catch {} }) } catch {}
      try { pc.close() } catch {}
    })
    pcsRef.current = {}
    // Stop and clear local media stream
    if (streamRef.current) {
      try { streamRef.current.getTracks().forEach((t) => t.stop()) } catch {}
    }
    streamRef.current = null
    // Close websocket
    try { wsRef.current?.close() } catch {}
    wsRef.current = null
    // Remove capture resize handler if set
    try { resizeHandlerRef.current && resizeHandlerRef.current() } catch {}
    resizeHandlerRef.current = null
    // Reset UI/state
    setPendingControlViewer(null)
    setConnected(false)
    setW(0)
    setH(0)
    // Update host status to inactive when ending session
    if (sessionId) {
      fetch(`${serverUrl}/api/viewer/updateHostStatus/${sessionId}/false`, { 
        method: 'POST' 
      }).catch(err => {
        console.error('Failed to update host status to inactive:', err)
      })
    }
    setSessionId('')
  }

  return (
    <div className="container">
      <h2>Host</h2>
      <div className="controls">
        <input placeholder="Session ID" value={sessionId} onChange={(e) => setSessionId(e.target.value)} />
        <button onClick={connectWs} disabled={!sessionId || connected}>Join & Connect</button>
        <button className='start-capture' onClick={startScreenShare} disabled={!connected}>Start Capture</button>
        <button onClick={endSession} disabled={!connected}>End Session</button>
        <span className="status">{connected ? 'Connected' : 'Disconnected'}</span>
      </div>

      <div className="status">{w} x {h}</div>
    </div>
  )
}

export default App
