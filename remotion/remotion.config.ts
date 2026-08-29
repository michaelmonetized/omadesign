import {Config} from "@remotion/cli/config";
Config.setVideoImageFormat("jpeg");
Config.setOverwriteOutput(true);
Config.setConcurrency(4);
Config.setChromiumOpenGlRenderer("swiftshader");
Config.setBrowserExecutable("/usr/bin/chromium");
