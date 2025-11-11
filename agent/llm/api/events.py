"""WebSocket routes for pushing agent events to Rust."""

import json
import logging
from fastapi import APIRouter, WebSocket, WebSocketDisconnect

from ...core.event_manager import event_manager
from ..schemas.analysis import AnalysisRequest
from ..services.strategy_analyzer import StrategyAnalyzer

router = APIRouter(prefix="/agent", tags=["agent"])
logger = logging.getLogger(__name__)


@router.websocket("/events/ws")
async def agent_event_socket(websocket: WebSocket) -> None:
    await event_manager.connect(websocket)
    analyzer = StrategyAnalyzer()
    
    try:
        while True:
            message = await websocket.receive_text()
            
            # 解析 JSON 消息
            try:
                data = json.loads(message)
                message_type = data.get("type")
                
                if message_type == "trigger_analysis":
                    logger.info("收到策略分析请求")
                    
                    # 执行分析
                    try:
                        result = await analyzer.analyze(AnalysisRequest())
                        
                        # 发送分析结果
                        response = {
                            "type": "analysis_result",
                            "analysis": {
                                "summary": result.summary,
                            }
                        }
                        await websocket.send_json(response)
                        logger.info("策略分析完成")
                        
                    except Exception as e:
                        logger.error(f"策略分析失败: {e}", exc_info=True)
                        error_response = {
                            "type": "analysis_error",
                            "error": str(e)
                        }
                        await websocket.send_json(error_response)
                else:
                    logger.warning(f"未知消息类型: {message_type}")
                    
            except json.JSONDecodeError:
                logger.warning(f"无法解析消息: {message}")
                
    except WebSocketDisconnect:
        logger.info("WebSocket 连接断开")
    finally:
        await event_manager.disconnect(websocket)
