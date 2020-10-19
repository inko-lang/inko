ruby -e @"
puts \"section_start:#{Time.now.utc.to_i}:choco_install[collapsed=true]\r\e[0K$args\"
"@

Invoke-Expression "$args"

ruby -e 'puts \"section_end:#{Time.now.utc.to_i}:choco_install\r\e[0K\"'
