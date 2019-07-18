#version 450

layout (set = 1, binding = 1) uniform texture2D texColor;
layout (set = 1, binding = 2) uniform sampler samplerColor;

layout(location = 0) in vec2 texCoord;

layout(location = 0) out vec4 outColor;

void main() {
    outColor = texture(sampler2D(texColor, samplerColor), texCoord);
}