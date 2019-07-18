#version 450

layout (binding = 0) uniform UniformBufferObject {
    mat4 transform;
} ubo;

layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec2 inTexCoord;

layout(location = 0) out vec2 textCoord;

void main() {
    gl_Position = ubo.transform * vec4(inPosition, 1.0);
    textCoord = inTexCoord;
}