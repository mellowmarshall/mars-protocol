import React from "react";
import { Composition } from "remotion";
import { MarsDemo } from "./MarsDemo";

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="MarsDemo"
        component={MarsDemo}
        durationInFrames={28 * 30} // 28 seconds at 30fps
        fps={30}
        width={1280}
        height={720}
      />
    </>
  );
};
