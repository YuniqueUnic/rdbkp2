import yaml
import os
import logging
from logging.handlers import RotatingFileHandler

class ServerConfig:
    def __init__(self, config_path='config.yaml'):
        self.config = self._load_config(config_path)
        self._setup_logging()
    
    def _load_config(self, config_path):
        if not os.path.exists(config_path):
            raise FileNotFoundError(f"Config file not found: {config_path}")
            
        with open(config_path, 'r') as f:
            return yaml.safe_load(f)
    
    def _setup_logging(self):
        if self.config['logging'].get('rotate', False):
            handler = RotatingFileHandler(
                self.log_file,
                maxBytes=self._parse_size(self.config['logging']['max_size']),
                backupCount=self.config['logging']['max_files']
            )
        else:
            os.makedirs(os.path.dirname(self.log_file), exist_ok=True)
            handler = logging.FileHandler(self.log_file)

        logging.basicConfig(
            level=getattr(logging, self.config['logging']['level'].upper()),
            format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
            handlers=[handler]
        )

    def _parse_size(self, size_str):
        units = {'K': 1024, 'M': 1024*1024, 'G': 1024*1024*1024}
        unit = size_str[-1].upper()
        if unit in units:
            return int(size_str[:-1]) * units[unit]
        return int(size_str)
    
    @property
    def port(self):
        return self.config['server']['port']
    
    @property
    def welcome_message(self):
        return self.config['server']['welcome_message']
    
    @property
    def version(self):
        return self.config['server']['version']
    
    @property
    def log_format(self):
        return self.config['logging']['format']
    
    @property
    def log_file(self):
        return self.config['logging']['file'] 