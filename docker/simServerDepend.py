import configparser
import os
import logging
from logging.handlers import RotatingFileHandler
from pathlib import Path

class ServerConfig:
    def __init__(self, config_path='./config/config.ini'):
        self.config = self._load_config(config_path)
    
    def _load_config(self, config_path):
        if not os.path.exists(config_path):
            raise FileNotFoundError(f"Config file not found: {config_path}")
            
        config = configparser.ConfigParser()
        config.read(config_path)
        return config
    
    def _setup_logging(self):
        if self.config.getboolean('logging', 'rotate'):
            handler = RotatingFileHandler(
                self.log_file,
                maxBytes=self._parse_size(self.config.get('logging', 'max_size')),
                backupCount=self.config.getint('logging', 'max_files')
            )
        else:
            os.makedirs(os.path.dirname(self.log_file), exist_ok=True)
            handler = logging.FileHandler(self.log_file)

        formatter = logging.Formatter(self.config.get('logging', 'format'))
        handler.setFormatter(formatter)
        logger = logging.getLogger('simserver')
        logger.addHandler(handler)
        logger.setLevel(getattr(logging, self.config.get('logging', 'level').upper()))

    def _parse_size(self, size_str):
        units = {'K': 1024, 'M': 1024*1024, 'G': 1024*1024*1024}
        unit = size_str[-1].upper()
        if unit in units:
            return int(size_str[:-1]) * units[unit]
        return int(size_str)
    
    @property
    def port(self):
        return self.config.getint('server', 'port')
    
    @property
    def welcome_message(self):
        return self.config.get('server', 'welcome_message')
    
    @property
    def version(self):
        return self.config.get('server', 'version')
    
    @property
    def log_format(self):
        return self.config.get('logging', 'format')
    
    @property
    def log_file(self):
        return self.config.get('logging', 'file') 