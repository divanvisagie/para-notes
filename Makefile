LAUNCHD_PLIST := $(HOME)/Library/LaunchAgents/com.divanv.para.serve.plist

.PHONY: install-daemon start-daemon stop-daemon restart-daemon uninstall-daemon

install-daemon:
	resources/launchd/install.sh

start-daemon:
	launchctl load -w $(LAUNCHD_PLIST)

stop-daemon:
	launchctl unload -w $(LAUNCHD_PLIST)

restart-daemon: stop-daemon start-daemon

uninstall-daemon:
	launchctl unload -w $(LAUNCHD_PLIST) || true
	rm -f $(LAUNCHD_PLIST)
