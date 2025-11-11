"""WebSocket routes for pushing agent events to Rust."""

from fastapi import APIRouter, WebSocket, WebSocketDisconnect

from ...core.event_manager import event_manager

router = APIRouter(prefix="/agent", tags=["agent"])


@router.websocket("/events/ws")
async def agent_event_socket(websocket: WebSocket) -> None:
    await event_manager.connect(websocket)
    try:
        while True:
            message = await websocket.receive_text()
            if message.lower() == "ping":
                await websocket.send_text("pong")
    except WebSocketDisconnect:
        pass
    finally:
        await event_manager.disconnect(websocket)
