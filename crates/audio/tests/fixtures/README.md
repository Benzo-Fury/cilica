far_speech.wav and near_speech.wav: `say -v Daniel|Samantha -o x.aiff "<text>"` then
`afconvert -f WAVE -d LEI16@16000 -c 1 x.aiff x.wav` (macOS). Used by tests/echo.rs.
