FROM nervos/fiber:0.9.0-rc7

RUN if command -v apk >/dev/null 2>&1; then \
      apk add --no-cache curl ca-certificates; \
    elif command -v apt-get >/dev/null 2>&1; then \
      apt-get update && apt-get install -y --no-install-recommends curl ca-certificates && rm -rf /var/lib/apt/lists/*; \
    else \
      echo "curl package manager not found; image must provide curl for vault funding"; \
    fi

COPY entrypoint.sh /liquidlane-fiber-entrypoint.sh
RUN chmod +x /liquidlane-fiber-entrypoint.sh

ENTRYPOINT ["/liquidlane-fiber-entrypoint.sh"]
