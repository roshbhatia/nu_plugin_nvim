Require nu
Set WaitTimeout 15s

Set Shell "nu"
Set Theme "Catppuccin Macchiato"
Set FontSize 16
Set Width 1400
Set Height 720
Set Padding 22
Set Framerate 20
Set TypingSpeed 25ms

Sleep 1s

Type "nuvim context | select mode cursor cwd"
Enter
Wait+Screen /cursor/
Sleep 2s

Type "nuvim buffers | select id path filetype modified"
Enter
Wait+Screen /README.md/
Sleep 2s

Type "nuvim diagnostics | select severity row column message"
Enter
Wait+Screen /severity/
Sleep 2s

Type "[{path: 'README.md', row: 2, column: 0, text: 'Improve the introduction', type: 'W'}] | nuvim quickfix set --title 'Documentation review'"
Enter
Wait+Screen /count/
Sleep 1s
Type "nuvim quickfix get | select path row column text"
Enter
Wait+Screen /Improve the introduction/
Sleep 2s

Ctrl+L
Sleep 1s
Type "[{service: 'api', state: 'healthy'}, {service: 'worker', state: 'deploying'}] | nuvim scratch --name deployments --filetype nuon | select id path filetype"
Enter
Wait+Screen /deployments/
Sleep 3s
