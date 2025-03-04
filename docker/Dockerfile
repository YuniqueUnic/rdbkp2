# Use Python alpine as base image - much smaller than slim
FROM python:3.9-alpine

# Set working directory
WORKDIR /app

# Copy the Python scripts
COPY simServer.py simServerDepend.py ./

# Create data directory
RUN mkdir config

# Copy the config
COPY config.ini config/

# Expose port 8666
EXPOSE 8666

# Replace the default Alpine mirror with the Alibaba Cloud mirror
RUN sed -i 's/dl-cdn.alpinelinux.org/mirrors.aliyun.com/g' /etc/apk/repositories

# Install dependencies
RUN apk add --no-cache curl

# Mount point for data volume
VOLUME ["/app/data"]

# Run the server
CMD ["python", "simServer.py"]