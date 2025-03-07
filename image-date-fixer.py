import argparse
from icecream import ic
import os
import sys
from datetime import datetime, timedelta, timezone
import logging
from setup_logging import setup_logging, log_level_int_from_str
import signal
import re
from typing import Optional
from PIL import Image
import uuid
from pathlib import Path
import subprocess
import shutil

exit_flag = False

"""
TODO: add overview like this:
+---------+---------+-----+---------+---------+--------+--------------+
| Folders | Files   | New | Updated | Removed | Errors | Elapsed time |
+---------+---------+-----+---------+---------+--------+--------------+
| 59028   | 1216570 | 0   | 1855    | 0       | 0      | 00:12:58     |
+---------+---------+-----+---------+---------+--------+--------------+
"""


def get_date_from_android_filepath(file_path: str) -> Optional[datetime]:
    """
    Extracts the date from Android-style
    filenames (e.g., IMG_20190818_130841.jpg).
    Returns None if no valid date is found.
    """
    filename = os.path.basename(file_path)
    match = re.search(r'IMG_(\d{8})_(\d{6})', filename)
    if match:
        date_str, time_str = match.groups()
        try:
            date = datetime.strptime(date_str + time_str, "%Y%m%d%H%M%S")
            return date
        except ValueError:
            return None
    return None


def get_date_from_whatsapp_filepath(file_path: str) -> Optional[datetime]:
    """
    Extracts the date from WhatsApp-style filenames (e.g., IMG-20250127-WA0006.jpg).
    Returns None if no valid date is found.
    """
    filename = os.path.basename(file_path)
    match = re.search(r'IMG-(\d{8})-WA\d+', filename)
    if match:
        date_str = match.group(1)
        try:
            date = datetime.strptime(date_str, "%Y%m%d")
            return date
        except ValueError:
            return None
    return None


def get_date_from_normal_date_prefixed_filepath(file_path: str) -> Optional[datetime]:
    """
    Extracts the date and optional time from filenames prefixed with a YYYY, YYYYMM, YYYYMMDD, YYYY-MM, or YYYY-MM-DD format.
    Optionally, a time in one of the following formats may follow:
    * in HHMMSS format (e.g., `-211056`)
    * 2019-07-14 20_25_57
    Returns None if no valid date is found.
    """
    filename = os.path.basename(file_path)
    match = re.match(
        r'^(\d{4})(?:-?(\d{2}))?(?:-?(\d{2}))?(?:-(\d{2})(\d{2})(\d{2}))?[\s\-_a-zA-z]', filename)
    if match:
        year_m, month_m, day_m, hour_m, minute_m, second_m = match.groups()

        # Default missing values to ensure a valid date-time
        if not year_m:
            return None
        year = year_m
        month = month_m if month_m else "01"
        day = day_m if month_m and day_m else "01"
        hour = hour_m if day_m and hour_m else "00"
        minute = minute_m if hour_m and minute_m else "00"
        second = second_m if minute_m and second_m else "00"

        date_str = f"{year}-{month}-{day} {hour}:{minute}:{second}"
        try:
            return datetime.strptime(date_str, "%Y-%m-%d %H:%M:%S")
        except ValueError:
            return None
    return None


def get_date_from_uuid_filepath(file_path: str) -> Optional[datetime]:
    """
    Extracts the date from timestamp prefixed UUID filenames.
    Returns None if no valid date is found.
    """
    filename = os.path.basename(file_path)
    # verify that the filename is timestamp prefixed UUID
    match = re.match(
        r'^(\d+)-([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})', filename)
    if match:
        timestamp, uuid_str = match.groups()
        try:
            date = datetime.fromtimestamp(int(timestamp)/1000)
            return date
        except ValueError:
            return None
    return None


def get_date_from_screenshot_prefixed_filepath(file_path: str) -> Optional[datetime]:
    """
    Extracts the date from filenames prefixed with `Screenshot_`.
    Returns None if no valid date is found.
    """
    prefixes = ["Screenshot_", "Screenshot-", "Screenshot "]
    filename = os.path.basename(file_path)
    for prefix in prefixes:
        if filename.startswith(prefix):
            filename_without_prefix = filename.replace(prefix, "")
            return get_date_from_filepath(os.path.join(os.path.dirname(file_path), filename_without_prefix))
    return None


def get_date_from_foldername(file_path: str) -> Optional[datetime]:
    """
    Extracts the date from the foldername.
    Returns None if no valid date is found.
    """
    folder_path = os.path.dirname(file_path)
    folder_date = get_date_from_filepath(folder_path)
    logging.debug(f"Using folder date: {folder_date} for {file_path}")
    return folder_date


def get_date_from_filepath(file_path: str) -> Optional[datetime]:
    """
    Extracts the date from the filename.
    Returns None if no valid date is found.
    """
    handler_functions = [
        get_date_from_whatsapp_filepath,
        get_date_from_android_filepath,
        get_date_from_normal_date_prefixed_filepath,
        get_date_from_uuid_filepath,
        get_date_from_screenshot_prefixed_filepath,
    ]

    for handler in handler_functions:
        date = handler(file_path)
        # discard date if it is in the future
        if date and date > datetime.now():
            logging.debug(
                f"Discarding extracted date {date} from {handler.__name__} as it is in the future.")
            date = None
        if date:
            return date

    return None


def get_exif_date(file: str) -> Optional[datetime]:
    """
    Extracts the EXIF date from the file.
    Returns None if no EXIF date is found.
    """
    try:
        with Image.open(file) as img:
            if not hasattr(img, "_getexif"):
                return None
            exif_data = img._getexif()
            if exif_data:
                date_str = exif_data.get(36867)  # Tag 36867: DateTimeOriginal
                if date_str:
                    return datetime.strptime(date_str, "%Y:%m:%d %H:%M:%S")
    except (OSError, ValueError, Image.DecompressionBombError):
        return None
    return None


def set_exif_date(file: str, date: datetime, config) -> None:
    """
    Sets the EXIF date of the file.
    """
    if config.dry_run:
        logging.debug(f"Would set EXIF date of {file} to {date}")
        return

    # use exiftool
    ret = subprocess.run(
        ["exiftool", "-DateTimeOriginal=" +
            date.strftime("%Y:%m:%d %H:%M:%S"), file]
    )
    if ret.returncode != 0:
        logging.error(f"Failed to set EXIF date of {file} to {date}")
    return None


def set_file_date(file: str, date: datetime, config) -> None:
    """
    Sets the modified date of the file.
    """
    if config.dry_run:
        logging.debug(f"Would set file date of {file} to {date}")
        return

    # do not set modification times earlier than 1970
    if date < datetime(1970, 1, 2):
        date = datetime(1970, 1, 2)

    os.utime(file, (date.timestamp(), date.timestamp()))


def get_file_date(file: str) -> datetime:
    """
    Returns the modified date of the file.
    """
    return datetime.fromtimestamp(os.path.getmtime(file))


def signal_handler(sig, frame):
    global exit_flag
    exit_flag = True

    # log the signal and exit
    logging.info(f"Received signal {sig}")
    logging.info("Exiting...")

    sys.exit(0)


def process_directory(directory, config):
    logging.debug(f"Processing directory: {directory}")
    for root, dirs, files in os.walk(directory):
        for file in files:
            if exit_flag:
                return
            file_path = os.path.join(root, file)
            if any(exclude_dir in file_path for exclude_dir in config.exclude_dirs):
                logging.debug(f"Skipping excluded directory: {file_path}")
                continue
            process_file(file_path, config)


def process_file(file, config):
    logging.info(f"Processing file: {file}")

    invalid_date_threshold = datetime.now() + timedelta(days=config.fix_future_dates)

    original_file_timestamp = get_file_date(file)

    # check if the file timestamp is in the future
    if config.fix_future_dates and original_file_timestamp > invalid_date_threshold:
        logging.info(
            f"File timestamp is in the future: {original_file_timestamp}. Setting to current time."
        )
        set_file_date(file, datetime.now(), config)

    # check if the file timestamp is before 2-1-1970
    if original_file_timestamp < datetime(1970, 1, 2):
        logging.info(
            f"File timestamp is before 2-1-1970: {original_file_timestamp}. Setting to 2-1-1970."
        )
        set_file_date(file, datetime(1970, 1, 2), config)

    # check that the file has an supported image extension
    if not file.lower().endswith((".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".tiff", ".tif", ".heic", ".heif", ".avif", ".jfif", ".jpe", ".jif", ".jfi", ".raw")):
        logging.debug("File is not an image. Skipping.")
        return

    # get the EXIF date
    original_exif_date = get_exif_date(file)

    if original_exif_date and original_exif_date < datetime(1970, 1, 2):
        logging.info(
            f"EXIF date is before 2-1-1970: {original_exif_date}. Setting to 2-1-1970."
        )
        set_exif_date(file, datetime(1970, 1, 2), config)
    elif config.fix_future_dates and original_exif_date and original_exif_date > invalid_date_threshold:
        logging.info(
            f"EXIF date is in the future: {original_exif_date}. Setting to current time."
        )
        set_exif_date(file, datetime.now(), config)
    # skip if the file has an EXIF date
    elif original_exif_date:
        logging.debug(f"File has an EXIF date: {original_exif_date}")
        return

    # get the date from the filename
    date = get_date_from_filepath(file)
    if not date:
        # get the date from the foldername
        date = get_date_from_foldername(file)

    if not date and not original_exif_date:
        logging.warning(f"Found no date resolution for {file}")
        return

    # check if the file timestamp year matches the date year
    # For this to be more accurate all handlers should return a confidence value
    if date.year == original_file_timestamp.year:
        logging.info(
            f"Only updating the exif data of {file} as the file timestamp year matches the extracted date."
        )
        # the file timestamp might be accurate, so we don't want to overwrite it
        set_exif_date(file, original_file_timestamp, config)
        return

    logging.info(f"Set {file} to {date}")
    # set the EXIF date
    set_exif_date(file, date, config)
    # set the file date
    set_file_date(file, date, config)


def main(config):
    if config.directory:
        process_directory(config.directory, config)
    elif config.file:
        process_file(config.file, config)


if __name__ == '__main__':
    description = """
    Welcome to image-date-fixer!
    This script extracts possible time stamp information from the filename and
    sets the exif and modified times accordingly.
    Existing exif times are never overwritten.
    It also corrects dates that are too far in the future to the present.
    """
    signal.signal(signal.SIGTERM, signal_handler)
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGQUIT, signal_handler)

    parser = argparse.ArgumentParser(description)
    input_files_group = parser.add_mutually_exclusive_group(required=True)
    input_files_group.add_argument(
        '--directory', type=str, help='Directory to process recursively')
    input_files_group.add_argument('--file', type=str, help='File to process')
    parser.add_argument(
        "--exclude-dirs",
        help="Directories to exclude",
        nargs="+",
        default=[],
    )
    parser.add_argument(
        "--log-level",
        help="Log level",
        choices=["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"],
    )
    parser.add_argument('--fix-future-dates', type=int,
                        help='Fix dates that are this many days in the future')
    parser.add_argument('--dry-run', action='store_true', help='Dry run')
    args = parser.parse_args()
    config = args
    setup_logging(config)

    # error if exiftool is not available
    if not shutil.which("exiftool"):
        logging.error("exiftool is required to run this script.")
        sys.exit(1)

    main(config)
