# Introduction
This script plots the heartrate collected via ANT+ dongle and compatible heartrate sensor. It can be run either directly as a python script or on Windows as an executable file.

![Plotter in action](/img/plotter.png)

# User-Configurable Settings
If a `settings.ini` file does not exist in the executable or script directory it will be created on first run. The file contains several configurable parameters:
- `backgroundcolor` matplotlib color for plot background
- `linecolor` matplotlib color for BPM line
- `labelscolor` matplotlib color for labels
- `spinescolor` matplotlib color for axes
- `xpts` number of datapoints displayed before X axis starts scrolling
- `intervalms` interval of funcanimation frames
- `ybottom` bottom y limit
- `ytop` top y limit
- `width` window width in inches
- `height` window height in inches
- `posx` window x position in pixels from top-left
- `posy` window y position in pixels from top-left

# Executable
This is built specifically for Windows. Simply double-click the `plotter.exe` file in the `dist` subdirectory.

# Direct Script
These instructions will be given for python3 running on Linux.
1. Install python3 and pip
2. Create the virtual environment by entering the project folder and doing `python3 -m venv venv`
3. Activate the virtual environment with `source venv/bin/activate`
4. Install the requirements with `pip install -r requirements.txt`
5. Run the script: `python3 plotter.py`

# Building for Windows
Note that the required packages will be installed in the virtual environment subdirectory, `.\venv\Lib\site-packages` as shown in the pathex within `plotter.spec`. The script needs to package the libusb0 driver which should be located at `C:\Windows\System32\libusb0.dll` as shown in the datas within `plotter.spec`.

1. Install python3, pip and pyinstaller
2. Create the virtual environment by entering the project folder and doing `python -m venv venv`
3. Activate the virtual environment with `venv\Scripts\activate`
4. Install the requirements with `pip install -r requirements.txt`
5. Build the executable by doing `pyinstaller plotter.spec` (do _not_ do `pyinstaller plotter.py` as this will overwrite the .spec file, wiping out the pathex and data references explained above)
6. The resulting executable will land in the `.\build` directory.

