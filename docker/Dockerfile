# Use Python alpine as base image - much smaller than slim
FROM python:3.9-alpine

# Set working directory
WORKDIR /app

# Copy the Python scripts and config
COPY simServer.py simServerDepend.py config.ini ./

# Create data directory
RUN mkdir data

# Expose port 8666
EXPOSE 8666

# Mount point for data volume
VOLUME ["/app/data"]

# Run the server
CMD ["python", "simServer.py"]
