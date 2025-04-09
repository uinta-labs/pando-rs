run-on ip:
    #!/usr/bin/env bash
    set -euo pipefail
    set -x

    host_arch=$(ssh -p 22222 \
      -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null \
       root@{{ ip }} uname -m)
    echo "Host architecture: $host_arch"

    docker_platform=""
    case $host_arch in
      x86_64)
        docker_platform="linux/amd64"
        ;;
      aarch64)
        docker_platform="linux/arm64"
        ;;
      *)
        echo "Unsupported host architecture: $host_arch"
        exit 1
        ;;
    esac

    new_image=$(docker build --platform $docker_platform -q -f Dockerfile .)
    echo "New image: $new_image"

    rm -f /tmp/new-container.tgz
    docker save $new_image > /tmp/new-container.tgz

    scp -P 22222 \
      -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null \
        /tmp/new-container.tgz \
     root@{{ ip }}:/tmp/new-container.tgz

    ssh -p 22222 \
      -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null \
         root@{{ ip }} 'image_id=$( balena load -i /tmp/new-container.tgz | cut -d : -f 3 ); balena tag ${image_id} pando-agent:preload && systemctl restart pando-agent.service'



db:
  #!/usr/bin/env bash
  set -euo pipefail

  docker kill pando-remote-db || true
  docker rm pando-remote-db || true
  docker run --rm -d \
    --name pando-remote-db \
    -p 54432:5432 \
    -e POSTGRES_USER=pando_service \
    -e POSTGRES_PASSWORD=hunter2 \
    -e POSTGRES_DB=pandodb \
    postgres:16-alpine