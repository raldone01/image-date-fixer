import logging


def log_level_int_from_str(log_level: str) -> int:
    """
    'DEBUG', 'INFO', 'WARNING', 'ERROR', 'CRITICAL'
    """
    log_mapping = logging.getLevelNamesMapping()
    return log_mapping[log_level.upper()]


def setup_logging(config_args) -> None:
    new_log_level = config_args.log_level
    if new_log_level is None:
        new_log_level = "INFO"
    log_level = log_level_int_from_str(new_log_level)
    logging.basicConfig(
        level=log_level, format="%(asctime)s %(levelname)s %(name)s %(message)s"
    )
