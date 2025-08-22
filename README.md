# Discord Rich Presence for Emacs on WSL

If you pretend to use this just remember to modify the path to dc-bridge.exe on elcord.el
```
(defun elcord--make-process ()
  "Make the asynchronous process that communicates with Discord IPC.
This FINAL version uses cmd.exe for maximum compatibility."
  (let* ((binary-win-path "PATH FOR YOUR dc-bridge.exe HERE")
         (final-command
          `("cmd.exe" "/c" , binary-win-path "0")))

    (message "Elcord: Starting final simplified command: %S" final-command)
    (make-process
     :name "*elcord-sock*"
     :command final-command
     :connection-type 'pipe
     :sentinel 'elcord--connection-sentinel
     :filter 'elcord--connection-filter
     :noquery t)))

```
