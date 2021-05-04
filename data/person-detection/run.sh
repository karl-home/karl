firejail --private=. \
--netfilter=/etc/firejail/karl.net \
--env=GLOBAL_HOOK_ID=person_detection \
--env=HOOK_ID=person_detection-2917637381 \
--env=TRIGGERED_TAG=camera.motion \
--env=PROCESS_TOKEN=7mb7zUL4s9Gs2F2WTe6v0qrFRemXuOgy \
--env=TRIGGERED_TIMESTAMP=2021-04-26T14:49:04.499119935-07:00 \
env/bin/python detect.py
