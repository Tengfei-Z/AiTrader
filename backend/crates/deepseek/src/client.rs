            // 记录发送的消息内容
            let messages_summary: Vec<String> = messages.iter().enumerate().map(|(idx, msg)| {
                let msg_json = serde_json::to_string(msg).unwrap_or_default();
                let preview = if msg_json.len() > 300 {
                    format!("{}... ({} chars)", &msg_json[..300], msg_json.len())
                } else {
                    msg_json
                };
                format!("msg[{}]: {}", idx, preview)
            }).collect();
            
            info!(
                turn,
                message_count = messages.len(),
                messages_detail = ?messages_summary,
                "Prepared messages for DeepSeek API"
            );

                // 使用 select! 强制超时，而不是 tokio::timeout
                let api_future = self.client.chat().create(chat_request.clone());
                let timeout_future = tokio::time::sleep(timeout_duration);
                
                // 看门狗：每 5 秒打印一次
                        for tick in 1..=9 {
                let api_result = tokio::select! {
                    result = api_future => {
                        watchdog.abort();
                        Some(result)
                    }
                    _ = timeout_future => {
                        watchdog.abort();
                        warn!(
                            turn,
                            retry,
                            elapsed_secs = api_call_start.elapsed().as_secs_f64(),
                            "DeepSeek API call timed out (forced by select!)"
                        );
                        None
                    }
                };
                    Some(Ok(resp)) => {
                        info!(
                            turn,
                            retry,
                            elapsed_secs = start_time.elapsed().as_secs_f64(),
                            api_call_secs = api_call_start.elapsed().as_secs_f64(),
                            "Received response from DeepSeek API"
                        );
                        response = Some(resp);
                        break;
                    }
                    Some(Err(e)) => {
                        warn!(
                            turn,
                            retry,
                            elapsed_secs = api_call_start.elapsed().as_secs_f64(),
                            error = %e,
                            error_debug = ?e,
                            "DeepSeek API call failed"
                        );
                        last_error = Some(anyhow::Error::from(e));
                    }
                    None => {
                        // 超时情况，已经在 select! 中记录了日志
                    }
            // 详细记录响应内容
            info!(
                turn,
                has_tool_calls = choice.message.tool_calls.is_some(),
                tool_calls_count = choice.message.tool_calls.as_ref().map(|t| t.len()).unwrap_or(0),
                has_content = choice.message.content.is_some(),
                content_preview = choice.message.content.as_ref().map(|c| {
                    if c.len() > 200 {
                        format!("{}... ({} chars)", &c[..200], c.len())
                    } else {
                        c.clone()
                    }
                }),
                finish_reason = ?choice.finish_reason,
                "Received DeepSeek response details"
            );

                        tool_call_id = %tool_call.id,
                        arguments_raw = %arguments_raw,
